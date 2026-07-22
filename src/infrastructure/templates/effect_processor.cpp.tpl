#include "effect_processor.h"

EffectProcessor::EffectProcessor(float sampleRate) : sampleRate_(sampleRate) {
    for (auto& knob : knobs_) {
        knob.store(0.0f, std::memory_order_relaxed);
    }
    for (auto& sw : switches_) {
        sw.store(0, std::memory_order_relaxed);
    }
    // Construct your dsp:: members here, e.g.:
    //   toneFilter_ = std::make_unique<dsp::BiquadFilter>();
}

void EffectProcessor::setKnob(uint32_t index, float value) {
    if (index < kNumKnobs) {
        knobs_[index].store(value, std::memory_order_relaxed);
    }
}

void EffectProcessor::setSwitch(uint32_t index, int32_t position) {
    if (index < kNumSwitches) {
        switches_[index].store(position, std::memory_order_relaxed);
    }
}

float EffectProcessor::processSample(float input) {
    // TODO: build your effect chain here, reading knobs_/switches_ as needed,
    // for example:
    //   float gain = knobs_[0].load(std::memory_order_relaxed);
    //
    // Passthrough by default (input = output).
    return input;
}
