#pragma once
#include "hothouse.h"
#include "effect_processor.h"
#include <atomic>

// Adapter: the ONLY place that knows about Hothouse / Daisy Seed. It translates
// hardware I/O to and from the platform-agnostic EffectProcessor, keeping the
// DSP core reusable (the firmware and the simulator share it).
class HothouseAdapter {
public:
    HothouseAdapter(clevelandmusicco::Hothouse& hw, EffectProcessor& dsp);

    // Reads knobs/switches and pushes them into the DSP. Call from the control loop.
    void updateControls();

    // Registered with Hothouse::StartAudio. Must be a static/free function
    // because Daisy expects a C-style function pointer.
    static void AudioCallback(daisy::AudioHandle::InputBuffer in,
                              daisy::AudioHandle::OutputBuffer out,
                              size_t size);

    // True bypass state, for external use (e.g. an LED).
    bool isBypassed() const { return bypassed_.load(std::memory_order_relaxed); }

private:
    clevelandmusicco::Hothouse& hw_;
    EffectProcessor& dsp_;

    // Instance bound for the static audio callback.
    static EffectProcessor* audioDsp_;
    // Shared between the control loop (writer) and audio callback (reader).
    static std::atomic<bool> bypassed_;
    daisy::Led led_bypass_;
};
