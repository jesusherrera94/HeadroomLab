#pragma once

// Reusable, platform-agnostic DSP building blocks live in the `dsp` namespace.
// Put your filters, clippers, DC blockers, oscillators, etc. here so that
// EffectProcessor can compose them.
//
// Keep this file free of Hothouse/Daisy headers: it must also build for the
// HeadroomLab simulator (the host `dylib` target), not just the firmware.
namespace dsp {

// Trivial starter primitive: returns its input unchanged. Replace or extend
// with your own building blocks (see the reference effects for examples such
// as BiquadFilter, FuzzEngine and DCBlocker).
inline float passthrough(float x) { return x; }

} // namespace dsp
