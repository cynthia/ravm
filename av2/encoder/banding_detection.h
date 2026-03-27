/*
 * Copyright (c) 2026, Alliance for Open Media. All rights reserved
 *
 * This source code is subject to the terms of the BSD 3-Clause Clear License
 * and the Alliance for Open Media Patent License 1.0. If the BSD 3-Clause Clear
 * License was not distributed with this source code in the LICENSE file, you
 * can obtain it at aomedia.org/license/software-license/bsd-3-c-c/.  If the
 * Alliance for Open Media Patent License 1.0 was not distributed with this
 * source code in the PATENTS file, you can obtain it at
 * aomedia.org/license/patent-license/.
 */
#ifndef AVM_AV2_COMMON_BANDING_DETECTION_H_
#define AVM_AV2_COMMON_BANDING_DETECTION_H_

#include "avm/avm_integer.h"
#include "av2/common/av2_common_int.h"
#include "av2/common/reconinter.h"
#include "av2/common/banding_metadata.h"

#ifdef __cplusplus
extern "C" {
#endif

/*!\brief Initialize banding detection
 *
 * Allocates working buffers and validates frame dimensions for CAMBI
 *
 * \param[in,out]  dbi          Banding detection information structure
 * \param[in]      frame_width  Width of the frame in pixels
 * \param[in]      frame_height Height of the frame in pixels
 * \param[in]      bit_depth    Bit depth of the frame (8 or 10)
 *
 * \return 1 on success, 0 if dimensions or bit depth are unsupported
 */
int avm_band_detection_init(Av2BandDetectInfo *const dbi, const int frame_width,
                            const int frame_height, const int bit_depth);

/*!\brief Free banding detection resources
 *
 * Releases all working buffers allocated by avm_band_detection_init
 *
 * \param[in,out]  dbi          Banding detection information structure
 */
void avm_band_detection_close(Av2BandDetectInfo *const dbi);

/*!\brief Assess banding via CAMBI
 *
 * Searches for presence of banding computing CAMBI on refernce and distorted
 *
 * \param[in]      frame        Compressed frame buffer
 * \param[in]      ref          Source frame buffer
 * \param[in,out]  cm           Pointer to top level common structure
 * \param[in]      dbi          Banding detection information structure
 * \param[out]     band_metadata   Banding hints metadata structure to be filled

 *
 * \return Nothing is returned. Instead, presence of banding is stored
 */
void avm_band_detection(const YV12_BUFFER_CONFIG *frame,
                        const YV12_BUFFER_CONFIG *ref,
                        Av2BandDetectInfo *const dbi, MACROBLOCKD *xd,
                        avm_banding_hints_metadata_t *band_metadata);

#ifdef __cplusplus
}  // extern "C"
#endif
#endif  // AVM_AV2_COMMON_BANDING_DETECTION_H_
