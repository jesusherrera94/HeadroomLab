# Project Name
TARGET = {{TARGET}}

# Uncomment to use LGPL (like ReverbSc, etc.)
USE_DAISYSP_LGPL=1

# Library Locations
HOTHOUSE_DIR ?= ../../HothouseExamples
LIBDAISY_DIR ?= $(HOTHOUSE_DIR)/libdaisy
DAISYSP_DIR ?= $(HOTHOUSE_DIR)/DaisySP

# Sources and Hothouse header files
CPP_SOURCES = $(TARGET).cpp hothouse_adapter.cpp effect_processor.cpp dsp_primitives.cpp $(HOTHOUSE_DIR)/src/hothouse.cpp
C_INCLUDES = -I$(HOTHOUSE_DIR)/src/

# The embedded Daisy/Hothouse build is only needed for the firmware target.
# Skip its makefiles when building the host `dylib`/`clean-dylib` target so the
# simulator library builds with just a host C++ compiler (no Hothouse toolchain).
DYLIB_GOALS := dylib clean-dylib
ifeq ($(filter $(DYLIB_GOALS),$(MAKECMDGOALS)),)
# Core location, and generic Makefile.
SYSTEM_FILES_DIR = $(LIBDAISY_DIR)/core
include $(SYSTEM_FILES_DIR)/Makefile

# Global includes
include $(HOTHOUSE_DIR)/src/Makefile
endif

# ── Host shared library target (simulator: .dylib / .so / .dll) ────────────
# Build with:  make dylib
CXX_HOST ?= clang++
ifeq ($(OS),Windows_NT)
    SHARED_EXT   := dll
    SHARED_FLAGS := -shared
else ifeq ($(shell uname -s),Darwin)
    SHARED_EXT   := dylib
    SHARED_FLAGS := -dynamiclib
else
    SHARED_EXT   := so
    SHARED_FLAGS := -shared -fPIC
endif
DYLIB_SRCS = hl_adapter.cpp effect_processor.cpp dsp_primitives.cpp
DYLIB_OUT  = build/lib$(TARGET).$(SHARED_EXT)

dylib:
	mkdir -p build
	$(CXX_HOST) -std=c++17 -O2 -fPIC $(SHARED_FLAGS) $(DYLIB_SRCS) -o $(DYLIB_OUT)

clean-dylib:
	rm -f $(DYLIB_OUT)

.PHONY: dylib clean-dylib
