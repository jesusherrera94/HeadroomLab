#include "hothouse_adapter.h"

using clevelandmusicco::Hothouse;
using daisy::AudioHandle;

EffectProcessor* HothouseAdapter::audioDsp_ = nullptr;
std::atomic<bool> HothouseAdapter::bypassed_{true};

HothouseAdapter::HothouseAdapter(Hothouse& hw, EffectProcessor& dsp)
    : hw_(hw), dsp_(dsp) {
    // Bind the instance for the static audio callback.
    audioDsp_ = &dsp;
    led_bypass_.Init(hw.seed.GetPin(Hothouse::LED_2), false);
}

void HothouseAdapter::updateControls() {
    // Refresh ADC + switch readings before mapping.
    hw_.ProcessAllControls();

    // Toggle bypass when FOOTSWITCH_2 is pressed.
    if (hw_.switches[Hothouse::FOOTSWITCH_2].RisingEdge()) {
        bypassed_.store(!bypassed_.load(std::memory_order_relaxed),
                        std::memory_order_relaxed);
    }

    // Map all 6 knobs (0..1) and 3 toggles (0=UP 1=MID 2=DOWN) into the DSP.
    dsp_.setKnob(0, hw_.GetKnobValue(Hothouse::KNOB_1));
    dsp_.setKnob(1, hw_.GetKnobValue(Hothouse::KNOB_2));
    dsp_.setKnob(2, hw_.GetKnobValue(Hothouse::KNOB_3));
    dsp_.setKnob(3, hw_.GetKnobValue(Hothouse::KNOB_4));
    dsp_.setKnob(4, hw_.GetKnobValue(Hothouse::KNOB_5));
    dsp_.setKnob(5, hw_.GetKnobValue(Hothouse::KNOB_6));

    dsp_.setSwitch(0, hw_.GetToggleswitchPosition(Hothouse::TOGGLESWITCH_1));
    dsp_.setSwitch(1, hw_.GetToggleswitchPosition(Hothouse::TOGGLESWITCH_2));
    dsp_.setSwitch(2, hw_.GetToggleswitchPosition(Hothouse::TOGGLESWITCH_3));

    // Update bypass LED state.
    led_bypass_.Set(bypassed_.load(std::memory_order_relaxed) ? 0.0f : 1.0f);
    led_bypass_.Update();
}

void HothouseAdapter::AudioCallback(AudioHandle::InputBuffer in,
                                    AudioHandle::OutputBuffer out,
                                    size_t size) {
    const bool bypass = bypassed_.load(std::memory_order_relaxed);

    for (size_t i = 0; i < size; ++i) {
        if (bypass) {
            // Buffered bypass: dry signal straight through (mono -> dual mono).
            out[0][i] = out[1][i] = in[0][i];
        } else {
            float processed = audioDsp_->processSample(in[0][i]);
            out[0][i] = out[1][i] = processed;
        }
    }
}
