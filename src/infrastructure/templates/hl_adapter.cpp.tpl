// HeadroomLab FFI contract for the simulator's dynamic library.
//
// These six `hl_*` symbols MUST stay in sync with the loader in HeadroomLab
// Build the shared library with:  make dylib

#include "effect_processor.h"
#include <cstddef>
#include <cstdint>

extern "C" {

void* hl_create(float sampleRate) {
    return new EffectProcessor(sampleRate);
}

void hl_destroy(void* plugin) {
    delete static_cast<EffectProcessor*>(plugin);
}

void hl_process(void* plugin, float* samples, size_t count,
                uint16_t channels, uint32_t sample_rate) {
    (void)channels;      // interleaved buffer; this starter is channel-agnostic
    (void)sample_rate;
    auto* dsp = static_cast<EffectProcessor*>(plugin);
    for (size_t i = 0; i < count; ++i) {
        samples[i] = dsp->processSample(samples[i]);
    }
}

void hl_set_knob(void* plugin, uint32_t index, float value) /* 0.0..1.0 */ {
    static_cast<EffectProcessor*>(plugin)->setKnob(index, value);
}

void hl_set_switch(void* plugin, uint32_t index, int32_t position) /* 0=UP 1=MID 2=DOWN */ {
    static_cast<EffectProcessor*>(plugin)->setSwitch(index, position);
}

void hl_set_footswitch(void* plugin, uint32_t index, bool pressed) {
    // Documented no-op: bypass is handled by HeadroomLab. Add behavior here if
    // you want the footswitches to drive your effect in the simulator.
    (void)plugin;
    (void)index;
    (void)pressed;
}

} // extern "C"
