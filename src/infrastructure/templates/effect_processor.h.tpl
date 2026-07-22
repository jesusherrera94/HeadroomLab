#pragma once
#include "dsp_primitives.h"
#include <atomic>
#include <cstdint>
#include <memory>

// Platform-agnostic DSP core for {{PROJECT_NAME}}.
//
// It knows nothing about Hothouse/Daisy or HeadroomLab — it just processes
// audio and exposes thread-safe control setters. Both the firmware
// (HothouseAdapter) and the simulator (hl_adapter.cpp) drive this same class,
// so your effect stays reusable across builds.
class EffectProcessor {
public:
    explicit EffectProcessor(float sampleRate);

    // --- Thread-safe control setters (called from the UI / control loop) ---
    // Generic indexed setters keep this starter effect-agnostic; rename or
    // specialize them (setGain, setTone, ...) as your effect takes shape.

    // 6 knobs, value in 0.0..1.0.
    void setKnob(uint32_t index, float value);
    // 3 three-way switches, position 0 = UP, 1 = MID, 2 = DOWN.
    void setSwitch(uint32_t index, int32_t position);

    // --- Audio processing (called from the audio callback) ---
    float processSample(float input);

private:
    static constexpr uint32_t kNumKnobs = 6;
    static constexpr uint32_t kNumSwitches = 3;

    const float sampleRate_;

    // Control state, stored atomically for a lock-free hand-off to the audio
    // thread. Read them inside processSample() with memory_order_relaxed.
    std::atomic<float> knobs_[kNumKnobs];
    std::atomic<int> switches_[kNumSwitches];

    // RAII member pattern for owned DSP primitives (no raw new). Add yours as
    // the effect grows, e.g.:
    //   std::unique_ptr<dsp::BiquadFilter> toneFilter_;
};
