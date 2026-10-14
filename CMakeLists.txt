# # CMake project for fdk-aac
#
# ## Basic usage
#
# 1. Download and install CMake >= 3.10 (CMake >= 3.15 is recommended)
# 2. Clone fdk-aac repository
# 3. In fdk-aac directory create and go to `build` subdirectory:
#
#    mkdir build
#    cd build
#
# 4. Run cmake to configure project with desired build type (Release|Debug):
#
#    cmake .. -DCMAKE_BUILD_TYPE=Release
#
#    Note CMake configuration without CMAKE_BUILD_TYPE option will not set build
#    type to some default and it is most likely is not what you want.
#
#    If you want to build static (default is shared library), add BUILD_SHARED_LIBS=OFF option:
#
#    #    cmake .. -DBUILD_SHARED_LIBS=OFF
#
#    You can combine options as well:
#
#    cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF
#
# 5. Run cmake to build project:
#
#    cmake --build . -c Release

cmake_minimum_required(VERSION 3.10)

# Policies

## Enables CMAKE_MSVC_RUNTIME_LIBRARY option support for CMake >= 3.15
## if you want to use a MSVC multi-threaded statically-linked runtime library
## If you enable it, CMake will build fdk-acc.dll without external dependencies.
##
## Example usage:
##
## cmake .. -DCMAKE_MSVC_RUNTIME_LIBRARY="MultiThreaded$<$<CONFIG:Debug>:Debug>
if(POLICY CMP0091)
  cmake_policy(SET CMP0091 NEW)
endif()

project(fdk-aac VERSION 3.0.0)

# Includes

include(CheckSymbolExists)
include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

if(CMAKE_VERSION VERSION_LESS 3.11)
  set(CPACK_PACKAGE_VERSION_MAJOR ${PROJECT_VERSION_MAJOR})
  set(CPACK_PACKAGE_VERSION_MINOR ${PROJECT_VERSION_MINOR})
  set(CPACK_PACKAGE_VERSION_PATCH ${PROJECT_VERSION_PATCH})
endif()
include(CPack)

# Options

option(BUILD_SHARED_LIBS "Build shared library" ON)
option(BUILD_PROGRAMS "Build extra utilities" OFF)
option(FDK_AAC_INSTALL_CMAKE_CONFIG_MODULE "Install CMake package configuration file" ON)
option(FDK_AAC_INSTALL_PKGCONFIG_MODULE "Install pkg-config .pc file" ON)

# Checks

## Check if math functions are in separate library (Most of Linux distros, maybe some other OSes)
check_symbol_exists(sin "math.h" HAVE_DEFAULT_MATH)
if(NOT HAVE_DEFAULT_MATH)
  set(orig_CMAKE_REQUIRED_LIBRARIES ${CMAKE_REQUIRED_LIBRARIES})
  list(APPEND CMAKE_REQUIRED_LIBRARIES "m")
  check_symbol_exists(sin "math.h" HAVE_LIBM)
  set(CMAKE_REQUIRED_LIBRARIES ${orig_CMAKE_REQUIRED_LIBRARIES})
endif()

# Library

## Sources

set(AACDEC_SRC
  libAACdec/src/FDK_delay.cpp
  libAACdec/src/aac_ram.cpp
  libAACdec/src/aac_rom.cpp
  libAACdec/src/aacdec_drc.cpp
  libAACdec/src/aacdec_hcr.cpp
  libAACdec/src/aacdec_hcr_bit.cpp
  libAACdec/src/aacdec_hcrs.cpp
  libAACdec/src/aacdec_pns.cpp
  libAACdec/src/aacdec_tns.cpp
  libAACdec/src/aacdecoder.cpp
  libAACdec/src/aacdecoder_lib.cpp
  libAACdec/src/block.cpp
  libAACdec/src/channel.cpp
  libAACdec/src/channelinfo.cpp
  libAACdec/src/conceal.cpp
  libAACdec/src/ldfiltbank.cpp
  libAACdec/src/pulsedata.cpp
  libAACdec/src/rvlc.cpp
  libAACdec/src/rvlcbit.cpp
  libAACdec/src/rvlcconceal.cpp
  libAACdec/src/stereo.cpp
  libAACdec/src/usacdec_ace_d4t64.cpp
  libAACdec/src/usacdec_ace_ltp.cpp
  libAACdec/src/usacdec_acelp.cpp
  libAACdec/src/usacdec_fac.cpp
  libAACdec/src/usacdec_lpc.cpp
  libAACdec/src/usacdec_lpd.cpp
  libAACdec/src/usacdec_rom.cpp
  libAACdec/src/aac_ram.h
  libAACdec/src/aac_rom.h
  libAACdec/src/aacdec_drc.h
  libAACdec/src/aacdec_drc_types.h
  libAACdec/src/aacdec_hcr.h
  libAACdec/src/aacdec_hcr_bit.h
  libAACdec/src/aacdec_hcr_types.h
  libAACdec/src/aacdec_hcrs.h
  libAACdec/src/aacdec_pns.h
  libAACdec/src/aacdec_tns.h
  libAACdec/src/aacdecoder.h
  libAACdec/src/block.h
  libAACdec/src/channel.h
  libAACdec/src/channelinfo.h
  libAACdec/src/conceal.h
  libAACdec/src/conceal_types.h
  libAACdec/src/FDK_delay.h
  libAACdec/src/ldfiltbank.h
  libAACdec/src/overlapadd.h
  libAACdec/src/pulsedata.h
  libAACdec/src/rvlc.h
  libAACdec/src/rvlc_info.h
  libAACdec/src/rvlcbit.h
  libAACdec/src/rvlcconceal.h
  libAACdec/src/stereo.h
  libAACdec/src/usacdec_ace_d4t64.h
  libAACdec/src/usacdec_ace_ltp.h
  libAACdec/src/usacdec_acelp.h
  libAACdec/src/usacdec_const.h
  libAACdec/src/usacdec_fac.h
  libAACdec/src/usacdec_lpc.h
  libAACdec/src/usacdec_lpd.h
  libAACdec/src/usacdec_rom.h)

set(AACENC_SRC
  libAACenc/src/aacEnc_ram.cpp
  libAACenc/src/aacEnc_rom.cpp
  libAACenc/src/aacenc.cpp
  libAACenc/src/aacenc_lib.cpp
  libAACenc/src/aacenc_pns.cpp
  libAACenc/src/aacenc_tns.cpp
  libAACenc/src/adj_thr.cpp
  libAACenc/src/band_nrg.cpp
  libAACenc/src/bandwidth.cpp
  libAACenc/src/bit_cnt.cpp
  libAACenc/src/bitenc.cpp
  libAACenc/src/block_switch.cpp
  libAACenc/src/channel_map.cpp
  libAACenc/src/chaosmeasure.cpp
  libAACenc/src/dyn_bits.cpp
  libAACenc/src/grp_data.cpp
  libAACenc/src/intensity.cpp
  libAACenc/src/line_pe.cpp
  libAACenc/src/ms_stereo.cpp
  libAACenc/src/noisedet.cpp
  libAACenc/src/pnsparam.cpp
  libAACenc/src/pre_echo_control.cpp
  libAACenc/src/psy_configuration.cpp
  libAACenc/src/psy_main.cpp
  libAACenc/src/qc_main.cpp
  libAACenc/src/quantize.cpp
  libAACenc/src/sf_estim.cpp
  libAACenc/src/spreading.cpp
  libAACenc/src/tonality.cpp
  libAACenc/src/transform.cpp
  libAACenc/src/aacenc.h
  libAACenc/src/aacenc_pns.h
  libAACenc/src/aacEnc_ram.h
  libAACenc/src/aacEnc_rom.h
  libAACenc/src/aacenc_tns.h
  libAACenc/src/adj_thr.h
  libAACenc/src/adj_thr_data.h
  libAACenc/src/band_nrg.h
  libAACenc/src/bandwidth.h
  libAACenc/src/bit_cnt.h
  libAACenc/src/bitenc.h
  libAACenc/src/block_switch.h
  libAACenc/src/channel_map.h
  libAACenc/src/chaosmeasure.h
  libAACenc/src/dyn_bits.h
  libAACenc/src/grp_data.h
  libAACenc/src/intensity.h
  libAACenc/src/interface.h
  libAACenc/src/line_pe.h
  libAACenc/src/ms_stereo.h
  libAACenc/src/noisedet.h
  libAACenc/src/pns_func.h
  libAACenc/src/pnsparam.h
  libAACenc/src/pre_echo_control.h
  libAACenc/src/psy_configuration.h
  libAACenc/src/psy_const.h
  libAACenc/src/psy_data.h
  libAACenc/src/psy_main.h
  libAACenc/src/qc_data.h
  libAACenc/src/qc_main.h
  libAACenc/src/quantize.h
  libAACenc/src/sf_estim.h
  libAACenc/src/spreading.h
  libAACenc/src/tns_func.h
  libAACenc/src/tonality.h
  libAACenc/src/transform.h)

set(ARITHCODING_SRC
  libArithCoding/src/ac_arith_coder.cpp)

set(DRCDEC_SRC
  libDRCdec/src/FDK_drcDecLib.cpp
  libDRCdec/src/drcDec_gainDecoder.cpp
  libDRCdec/src/drcDec_reader.cpp
  libDRCdec/src/drcDec_rom.cpp
  libDRCdec/src/drcDec_selectionProcess.cpp
  libDRCdec/src/drcDec_tools.cpp
  libDRCdec/src/drcGainDec_init.cpp
  libDRCdec/src/drcGainDec_preprocess.cpp
  libDRCdec/src/drcGainDec_process.cpp
  libDRCdec/src/drcDec_gainDecoder.h
  libDRCdec/src/drcDec_reader.h
  libDRCdec/src/drcDec_rom.h
  libDRCdec/src/drcDec_selectionProcess.h
  libDRCdec/src/drcDec_tools.h
  libDRCdec/src/drcDec_types.h
  libDRCdec/src/drcDecoder.h
  libDRCdec/src/drcGainDec_init.h
  libDRCdec/src/drcGainDec_preprocess.h
  libDRCdec/src/drcGainDec_process.h)

set(FDK_SRC
  libFDK/src/FDK_bitbuffer.cpp
  libFDK/src/FDK_core.cpp
  libFDK/src/FDK_crc.cpp
  libFDK/src/FDK_decorrelate.cpp
  libFDK/src/FDK_hybrid.cpp
  libFDK/src/FDK_lpc.cpp
  libFDK/src/FDK_matrixCalloc.cpp
  libFDK/src/FDK_qmf_domain.cpp
  libFDK/src/FDK_tools_rom.cpp
  libFDK/src/FDK_trigFcts.cpp
  libFDK/src/autocorr2nd.cpp
  libFDK/src/dct.cpp
  libFDK/src/fft.cpp
  libFDK/src/fft_rad2.cpp
  libFDK/src/fixpoint_math.cpp
  libFDK/src/huff_nodes.cpp
  libFDK/src/mdct.cpp
  libFDK/src/nlc_dec.cpp
  libFDK/src/qmf.cpp
  libFDK/src/scale.cpp)

set(MPEGTPDEC_SRC
  libMpegTPDec/src/tpdec_adif.cpp
  libMpegTPDec/src/tpdec_adts.cpp
  libMpegTPDec/src/tpdec_asc.cpp
  libMpegTPDec/src/tpdec_drm.cpp
  libMpegTPDec/src/tpdec_latm.cpp
  libMpegTPDec/src/tpdec_lib.cpp
  libMpegTPDec/src/tp_version.h
  libMpegTPDec/src/tpdec_adif.h
  libMpegTPDec/src/tpdec_adts.h
  libMpegTPDec/src/tpdec_drm.h
  libMpegTPDec/src/tpdec_latm.h)

set(MPEGTPENC_SRC
  libMpegTPEnc/src/tpenc_adts.cpp
  libMpegTPEnc/src/tpenc_asc.cpp
  libMpegTPEnc/src/tpenc_lib.cpp
  libMpegTPEnc/src/tp_version.h
  libMpegTPEnc/src/tpenc_adts.h
  libMpegTPEnc/src/tpenc_asc.h)

set(PCMUTILS_SRC
  libPCMutils/src/limiter.cpp
  libPCMutils/src/pcm_utils.cpp
  libPCMutils/src/pcmdmx_lib.cpp
  libPCMutils/src/version.h)

set(SACDEC_SRC
  libSACdec/src/sac_bitdec.cpp
  libSACdec/src/sac_calcM1andM2.cpp
  libSACdec/src/sac_dec.cpp
  libSACdec/src/sac_dec_conceal.cpp
  libSACdec/src/sac_dec_lib.cpp
  libSACdec/src/sac_process.cpp
  libSACdec/src/sac_qmf.cpp
  libSACdec/src/sac_reshapeBBEnv.cpp
  libSACdec/src/sac_rom.cpp
  libSACdec/src/sac_smoothing.cpp
  libSACdec/src/sac_stp.cpp
  libSACdec/src/sac_tsd.cpp
  libSACdec/src/sac_bitdec.h
  libSACdec/src/sac_calcM1andM2.h
  libSACdec/src/sac_dec.h
  libSACdec/src/sac_dec_conceal.h
  libSACdec/src/sac_dec_interface.h
  libSACdec/src/sac_dec_ssc_struct.h
  libSACdec/src/sac_process.h
  libSACdec/src/sac_qmf.h
  libSACdec/src/sac_reshapeBBEnv.h
  libSACdec/src/sac_rom.h
  libSACdec/src/sac_smoothing.h
  libSACdec/src/sac_stp.h
  libSACdec/src/sac_tsd.h)

# Stripped: SACENC_SRC removed (AAC-LC only).

set(SBRDEC_SRC
  libSBRdec/src/HFgen_preFlat.cpp
  libSBRdec/src/env_calc.cpp
  libSBRdec/src/env_dec.cpp
  libSBRdec/src/env_extr.cpp
  libSBRdec/src/hbe.cpp
  libSBRdec/src/huff_dec.cpp
  libSBRdec/src/lpp_tran.cpp
  libSBRdec/src/psbitdec.cpp
  libSBRdec/src/psdec.cpp
  libSBRdec/src/psdec_drm.cpp
  libSBRdec/src/psdecrom_drm.cpp
  libSBRdec/src/pvc_dec.cpp
  libSBRdec/src/sbr_deb.cpp
  libSBRdec/src/sbr_dec.cpp
  libSBRdec/src/sbr_ram.cpp
  libSBRdec/src/sbr_rom.cpp
  libSBRdec/src/sbrdec_drc.cpp
  libSBRdec/src/sbrdec_freq_sca.cpp
  libSBRdec/src/sbrdecoder.cpp
  libSBRdec/src/env_calc.h
  libSBRdec/src/env_dec.h
  libSBRdec/src/env_extr.h
  libSBRdec/src/hbe.h
  libSBRdec/src/HFgen_preFlat.h
  libSBRdec/src/huff_dec.h
  libSBRdec/src/lpp_tran.h
  libSBRdec/src/psbitdec.h
  libSBRdec/src/psdec.h
  libSBRdec/src/psdec_drm.h
  libSBRdec/src/pvc_dec.h
  libSBRdec/src/sbr_deb.h
  libSBRdec/src/sbr_dec.h
  libSBRdec/src/sbr_ram.h
  libSBRdec/src/sbr_rom.h
  libSBRdec/src/sbrdec_drc.h
  libSBRdec/src/sbrdec_freq_sca.h
  libSBRdec/src/transcendent.h)

# Stripped: SBRENC_SRC removed (AAC-LC only).

set(SYS_SRC
  libSYS/src/genericStds.cpp
  libSYS/src/syslib_channelMapDescr.cpp)

set(fdk_aacinclude_HEADERS
  libSYS/include/machine_type.h
  libSYS/include/genericStds.h
  libSYS/include/FDK_audio.h
  libSYS/include/syslib_channelMapDescr.h
  libAACenc/include/aacenc_lib.h
  libAACdec/include/aacdecoder_lib.h)

set(libfdk_aac_SOURCES
    ${AACDEC_SRC} ${AACENC_SRC}
    ${ARITHCODING_SRC}
    ${DRCDEC_SRC}
    ${MPEGTPDEC_SRC} ${MPEGTPENC_SRC}
    ${SACDEC_SRC}
    ${SBRDEC_SRC}
    ${PCMUTILS_SRC} ${FDK_SRC} ${SYS_SRC})
# Stripped: SACENC_SRC, SBRENC_SRC removed (AAC-LC only).


list(TRANSFORM libfdk_aac_SOURCES PREPEND cpp/)
list(TRANSFORM fdk_aacinclude_HEADERS PREPEND cpp/)


if(WIN32 AND BUILD_SHARED_LIBS)
  set(libfdk_aac_SOURCES ${libfdk_aac_SOURCES} fdk-aac.def)
  endif()

## Create Library target. Actually fdk-aac is enough, but we also create
## FDK-AAC::fdk-aac for consistence with config-file package.

add_library(fdk-aac ${libfdk_aac_SOURCES})
add_library(FDK-AAC::fdk-aac ALIAS fdk-aac)

## Library target configuration

### Library target includes
target_include_directories(fdk-aac
  PUBLIC
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/cpp/libAACdec/include>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/cpp/libAACenc/include>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/cpp/libSYS/include>
  PRIVATE
    cpp/libArithCoding/include
    cpp/libDRCdec/include
    cpp/libSACdec/include
    cpp/libSBRdec/include
    cpp/libMpegTPDec/include
    cpp/libMpegTPEnc/include
    cpp/libFDK/include
    cpp/libPCMutils/include)

### Link math library if required
target_link_libraries(fdk-aac PRIVATE $<$<BOOL:${HAVE_LIBM}>:m>)

### Set public headers and shared library version. Version info is critical for Unix-like OSes.
set_target_properties(fdk-aac PROPERTIES
  PUBLIC_HEADER "${fdk_aacinclude_HEADERS}"
  VERSION 2.0.4
  SOVERSION 2
  MACHO_COMPATIBILITY_VERSION 3.0.0
  MACHO_CURRENT_VERSION 3.4.0
  LINKER_LANGUAGE C)
### Some compiler options from Makefile.am
if(MSVC)
  target_compile_options(fdk-aac PRIVATE /EHsc)
else()
  target_compile_options(fdk-aac PRIVATE -fno-exceptions -fno-rtti)
endif()

### Set proper name for MinGW or Cygwin DLL

if((MINGW OR CYGWIN) AND BUILD_SHARED_LIBS)
  set_property(TARGET fdk-aac PROPERTY RUNTIME_OUTPUT_NAME fdk-aac-2)
endif()

## Library installation

### Note we export `fdk-aac-targets` to use with config-file package.
install(TARGETS fdk-aac EXPORT fdk-aac-targets
  RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR} COMPONENT BIN
  ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR} COMPONENT DEV
  LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR} COMPONENT DEV
  PUBLIC_HEADER DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/fdk-aac COMPONENT DEV)

### Configure and install pkg-config module.
if(FDK_AAC_INSTALL_PKGCONFIG_MODULE)
  set(prefix ${CMAKE_INSTALL_PREFIX})
  set(exec_prefix "\$\{prefix\}")
  set(libdir "\$\{exec_prefix\}/${CMAKE_INSTALL_LIBDIR}")
  set(includedir "\$\{prefix\}/${CMAKE_INSTALL_INCLUDEDIR}")
  set(PACKAGE_VERSION ${PROJECT_VERSION})
  if(HAVE_LIBM)
    if(BUILD_SHARED_LIBS)
      set(LIBS_PRIVATE "-lm")
    else()
      set(LIBS_PUBLIC "-lm")
    endif()
  endif()
  configure_file(fdk-aac.pc.in fdk-aac.pc @ONLY)
  install(
    FILES
      ${CMAKE_CURRENT_BINARY_DIR}/fdk-aac.pc
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/pkgconfig)
endif()

### Configure and install config-file package. Something like pkg-config module, but for Windows.
if(FDK_AAC_INSTALL_CMAKE_CONFIG_MODULE)

  set(FDK_AAC_INSTALL_PACKAGEDIR ${CMAKE_INSTALL_LIBDIR}/cmake/fdk-aac)

  configure_package_config_file(fdk-aac-config.cmake.in ${CMAKE_CURRENT_BINARY_DIR}/fdk-aac-config.cmake
    INSTALL_DESTINATION ${FDK_AAC_INSTALL_PACKAGEDIR})

  if(CMAKE_VERSION VERSION_LESS 3.11)
    write_basic_package_version_file(fdk-aac-config-version.cmake COMPATIBILITY SameMajorVersion)
  else()
    write_basic_package_version_file(fdk-aac-config-version.cmake COMPATIBILITY SameMinorVersion)
  endif()

  install(EXPORT fdk-aac-targets
    NAMESPACE FDK-AAC::
    DESTINATION ${FDK_AAC_INSTALL_PACKAGEDIR})
  install(
    FILES
      ${CMAKE_CURRENT_BINARY_DIR}/fdk-aac-config.cmake
      ${CMAKE_CURRENT_BINARY_DIR}/fdk-aac-config-version.cmake
    DESTINATION ${FDK_AAC_INSTALL_PACKAGEDIR})
endif()

# Programs

if(BUILD_PROGRAMS)

  ## Program sources

  set(aac_enc_SOURCES
    aac-enc.c
    wavreader.c
    wavreader.h)

  set(aac_dec_SOURCES
    aac-dec.c
    wavwriter.c
    wavwriter.h)

  ## Program target
  add_executable(aac-enc ${aac_enc_SOURCES})
  add_executable(aac-dec ${aac_dec_SOURCES})

  ## Program target configuration
  target_link_libraries(aac-enc PRIVATE fdk-aac)
  target_link_libraries(aac-dec PRIVATE fdk-aac)
  target_compile_definitions(aac-enc PRIVATE $<$<BOOL:${MSVC}>:_CRT_SECURE_NO_WARNINGS>)
  target_compile_definitions(aac-dec PRIVATE $<$<BOOL:${MSVC}>:_CRT_SECURE_NO_WARNINGS>)
  target_include_directories(aac-enc PRIVATE cpp)
  target_include_directories(aac-dec PRIVATE cpp)
  if(WIN32)
    target_sources(aac-enc PRIVATE win32/getopt.h)
    target_sources(aac-dec PRIVATE win32/getopt.h)
    target_include_directories(aac-enc PRIVATE win32)
    target_include_directories(aac-dec PRIVATE win32)
  endif()

  ## Program target installation
  install(TARGETS aac-enc RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
  install(TARGETS aac-dec RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})


  ## Tool for testing the library
  set(test_encode_decode_SOURCES
    test-encode-decode.c
    wavreader.c
    wavreader.h
    sha1.c
    sha1.h)

  add_executable(test-encode-decode ${test_encode_decode_SOURCES})
  target_link_libraries(test-encode-decode PRIVATE fdk-aac)
endif()
