/*
 * Copyright (c) 2024, Alliance for Open Media. All rights reserved
 *
 * This source code is subject to the terms of the BSD 3-Clause Clear License
 * and the Alliance for Open Media Patent License 1.0. If the BSD 3-Clause Clear
 * License was not distributed with this source code in the LICENSE file, you
 * can obtain it at aomedia.org/license/software-license/bsd-3-c-c/.  If the
 * Alliance for Open Media Patent License 1.0 was not distributed with this
 * source code in the PATENTS file, you can obtain it at
 * aomedia.org/license/patent-license/.
 */

#include "av1/encoder/part_split_prune_tflite.h"

#include <cstdio>
#include <memory>
#include <mutex>
#include <unordered_map>
#include <iostream>

#include "common/tf_lite_includes.h"

#include "av1/encoder/simple_intrapred_tflite_model_128x128.h"
#include "av1/encoder/simple_intrapred_tflite_model_16x16.h"
#include "av1/encoder/simple_intrapred_tflite_model_32x32.h"
#include "av1/encoder/simple_intrapred_tflite_model_64x64.h"
#include "av1/encoder/sms_part_split_prune_tflite_model.h"
#include "av1/encoder/sms_part_none_prune_tflite_model.h"
#include "av1/encoder/sms_part_none_prune_rect_tflite_model.h"

#if HAVE_FEXCEPT
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <fenv.h>
#endif

typedef std::unique_ptr<TfLiteDelegate, decltype(&TfLiteXNNPackDelegateDelete)>
    TfLiteDelegateType;

struct Context {
  std::unique_ptr<tflite::Interpreter> models[MODEL_COUNT];
  bool old_model[MODEL_COUNT];
  uint8_t input_order[MODEL_COUNT][8]; // 8 inputs max
  std::vector<TfLiteDelegateType> to_delete;
};

std::mutex tfliteMutex;

static std::unique_ptr<tflite::Interpreter> create_interpreter(
    unsigned char *model_def, std::vector<TfLiteDelegateType> &to_delete) {
  std::lock_guard<std::mutex> lock(tfliteMutex);
  tflite::LoggerOptions::SetMinimumLogSeverity(tflite::TFLITE_LOG_ERROR);
  tflite::Model *model = (tflite::Model *)tflite::GetModel(model_def);

  const int num_threads = 1;
  TfLiteXNNPackDelegateOptions xnnpack_options =
      TfLiteXNNPackDelegateOptionsDefault();
  xnnpack_options.num_threads = AOMMAX(num_threads, 1);
  TfLiteDelegateType xnnpack_delegate(
      TfLiteXNNPackDelegateCreate(&xnnpack_options),
      &TfLiteXNNPackDelegateDelete);

  tflite::MutableOpResolver resolver;
  RegisterSelectedOps(&resolver);

  tflite::InterpreterBuilder builder(model, resolver);
  tflite::ErrorReporter *reporter(tflite::DefaultErrorReporter());
  std::unique_ptr<tflite::Interpreter> interpreter;
  builder(&interpreter);
  if (interpreter->ModifyGraphWithDelegate(xnnpack_delegate.get()) !=
      kTfLiteOk) {
    reporter->Report("Failed at modifying graph with XNNPack delegate");
    exit(1);
  }

  if (interpreter->AllocateTensors() != kTfLiteOk) {
    reporter->Report("Failed at allocating tensors");
    exit(1);
  }

  to_delete.push_back(std::move(xnnpack_delegate));
  return interpreter;
}

struct ModelDef {
  unsigned char *model_def;
  size_t model_size;
  const struct InputSpec input_spec;
  MODEL_TYPE type;
  const char *var_name;
  const char *enum_name;
  int part_type;
  int n_features;
  int model_version;
};

#define MODELDEF(data, type, part_type, n_features, model_version) \
  { data, sizeof(data), {false, NULL, NULL, NULL}, type, #data, #type, part_type, n_features, model_version }
#define MODELDEF_INS(data, type, part_type, n_features, model_version) \
  { data, sizeof(data), {true, data##_mean, data##_std, data##_std_inv}, type, #data, #type, part_type, n_features, model_version }

enum {
  PT_INVAL = -1, PT_NONE = 0, PT_SPLIT, PT_VERT, PT_HORZ
};

const ModelDef models[] = {
  MODELDEF(NULL, MODEL_OTHER, PT_INVAL, 0, 0),
  MODELDEF(a3_qp96_128_160_luma_BLOCK_128X128_intra_tflite, MODEL_128X128, PT_SPLIT, 39, 0),
  MODELDEF(a3_qp96_128_160_luma_BLOCK_64X64_intra_tflite, MODEL_64X64, PT_SPLIT, 39, 0),
  MODELDEF(a3_qp96_128_160_luma_BLOCK_32X32_intra_tflite, MODEL_32X32, PT_SPLIT, 39, 0),
  MODELDEF(a3_qp96_128_160_luma_BLOCK_16X16_intra_tflite, MODEL_16X16, PT_SPLIT, 39, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs12_110, MODEL_INTER_NONE_64X64_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs12_135, MODEL_INTER_NONE_64X64_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_64X64_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs9_110, MODEL_INTER_NONE_32X32_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs9_135, MODEL_INTER_NONE_32X32_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_32X32_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs6_110, MODEL_INTER_NONE_16X16_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs6_135, MODEL_INTER_NONE_16X16_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_16X16_160, PT_NONE, 66, 0),
  MODELDEF(NULL, MODEL_INTER_NONE_8X8_110, PT_NONE, 66, 0),
  MODELDEF(NULL, MODEL_INTER_NONE_8X8_135, PT_NONE, 66, 0),
  MODELDEF(NULL, MODEL_INTER_NONE_8X8_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs11_110, MODEL_INTER_NONE_BS11_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs11_135, MODEL_INTER_NONE_BS11_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_BS11_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs10_110, MODEL_INTER_NONE_BS10_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs10_135, MODEL_INTER_NONE_BS10_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_BS10_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs8_110, MODEL_INTER_NONE_BS8_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs8_135, MODEL_INTER_NONE_BS8_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_BS8_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs7_110, MODEL_INTER_NONE_BS7_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs7_135, MODEL_INTER_NONE_BS7_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_BS7_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs5_110, MODEL_INTER_NONE_BS5_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs5_135, MODEL_INTER_NONE_BS5_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_BS5_160, PT_NONE, 66, 0),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs4_110, MODEL_INTER_NONE_BS4_110, PT_NONE, 66, 6),
  MODELDEF_INS(sms_part_none_prune_tflite_model_bs4_135, MODEL_INTER_NONE_BS4_135, PT_NONE, 66, 6),
  MODELDEF(NULL, MODEL_INTER_NONE_BS4_160, PT_NONE, 66, 0),
  MODELDEF(sms_part_split_prune_tflite_model_bs12, MODEL_INTER_SPLIT_64X64, PT_SPLIT, 66, 0),
  MODELDEF(sms_part_split_prune_tflite_model_bs9, MODEL_INTER_SPLIT_32X32, PT_SPLIT, 66, 0),
  MODELDEF(sms_part_split_prune_tflite_model_bs6, MODEL_INTER_SPLIT_16X16, PT_SPLIT, 66, 0),
  MODELDEF(sms_part_split_prune_tflite_model_bs3, MODEL_INTER_SPLIT_8X8, PT_SPLIT, 66, 0),
};

static void dump_model(ModelDef *def) {
  char buf[128];
  sprintf(buf, "model_%d_%s.tflite", def->type, def->enum_name);
  printf("Storing %s\n", buf);
  FILE *fp = fopen(buf, "wb");
  fwrite(def->model_def, 1, def->model_size, fp);
  fclose(fp);
}

static void dump_input_spec(ModelDef *def) {
  if (!def->input_spec.valid)
    return;
  char buf[128];
  sprintf(buf, "model_%d_%s_input_spec.py", def->type, def->enum_name);
  printf("Storing %s\n", buf);
  FILE *fp = fopen(buf, "wb");
  fprintf(fp, "MEAN = [");
  for (int i = 0; i < def->n_features; i++)
    fprintf(fp, "%e, ", def->input_spec.mean[i]);
  fprintf(fp, "]\nSTD = [");
  for (int i = 0; i < def->n_features; i++)
    fprintf(fp, "%e, ", def->input_spec.std[i]);
  fprintf(fp, "]\n");
  fclose(fp);
}

static void get_input_order(tflite::Interpreter* interpreter, uint8_t* order) {
  static const std::unordered_map<std::string, uint8_t> order_lut = {
      {"serving_default_input:0", 0},
      {"serving_default_input_1:0", 0},
      {"serving_default_input_2:0", 1},
      {"serving_default_input_3:0", 2},
      {"serving_default_input_4:0", 3},
  };
  memset(order, 0, 8 * sizeof(order[0]));

  for (int i = 0; i < interpreter->inputs().size(); ++i) {
    int input_index = interpreter->inputs()[i];
    const TfLiteTensor* input_tensor = interpreter->tensor(input_index);
    try {
      auto value = order_lut.at(input_tensor->name);
      order[value] = input_index;
    } catch (const std::out_of_range& oor) {
      std::cout << "Model with unsupported input name: "
                << input_tensor->name << std::endl;
      exit(-1);
    }
  }
  printf("input order: %d,%d,%d,%d\n", order[0], order[1], order[2], order[3]);
}

static void ensure_tflite_init(void **context, MODEL_TYPE model_type) {
  assert(model_type != MODEL_OTHER);

  if (*context == nullptr) *context = new Context();
  Context *ctx = (Context *)*context;
  ModelDef def = models[model_type];
  if (!ctx->models[model_type]) {
    if (def.model_def != NULL) {
      ctx->models[model_type] = create_interpreter(def.model_def, ctx->to_delete);
      get_input_order(ctx->models[model_type].get(), &ctx->input_order[model_type][0]);
      dump_model(&def);
      dump_input_spec(&def);
    } else {
      printf("\x1b[91mUsing undefined model: %s(%d)\x1b[0m\n",
             models[model_type].enum_name, model_type);
    }
  }
}

extern "C" int av2_model_input_spec(MODEL_TYPE model_type,
                                    struct InputSpec *input_spec) {
  assert(model_type != MODEL_OTHER);
  ModelDef def = models[model_type];
  *input_spec = def.input_spec;
  return 0;
}

#if HAVE_FEXCEPT && CONFIG_DEBUG
#define FLOATING_POINT_DISABLE_EXCEPTIONS \
  const int float_excepts = fedisableexcept(FE_UNDERFLOW | FE_OVERFLOW);
#define FLOATING_POINT_RESTORE_EXCEPTIONS feenableexcept(float_excepts);
#else
#define FLOATING_POINT_DISABLE_EXCEPTIONS
#define FLOATING_POINT_RESTORE_EXCEPTIONS
#endif  // HAVE_FEXCEPT && CONFIG_DEBUG

// Simple intra ML TFLite based inference

static inline float norm(const float* input, int feature, struct InputSpec spec) {
  return spec.valid ?
      (input[feature] - spec.mean[feature]) * spec.invstd[feature] :
      input[feature];
}

extern "C" int av2_part_prune_tflite_exec(void **context, const float *ml_input,
                                          float *ml_output, MODEL_TYPE model_type) {
  assert(model_type != MODEL_OTHER);

  ensure_tflite_init(context, model_type);
  Context *ctx = (Context *)*context;
  tflite::Interpreter *interpreter = ctx->models[model_type].get();
  tflite::ErrorReporter *reporter(tflite::DefaultErrorReporter());

  int model_version = models[model_type].model_version;
  struct InputSpec input_spec = models[model_type].input_spec;
  int input_len = models[model_type].n_features;
  int output_len = 1;

  if (model_version == 0) {
    const TfLiteTensor *input_tensor = interpreter->input_tensor(0);
    int num_input_features = input_tensor->dims->data[1];
    if (num_input_features > input_len) {
      printf("\x1b[91mERROR:\x1b[0m Not enough input features: %d>%d\n",
             num_input_features, input_len);
      exit(1);
    }
    if (num_input_features != input_len && !ctx->old_model[model_type]) {
      printf("\x1b[95mWARN:\x1b[0m Too many input features for model %s: %d<%d"
             " (is it an old model?)\n", get_model_name(model_type),
             num_input_features, input_len);
      ctx->old_model[model_type] = true;
    }
    float *input = interpreter->typed_input_tensor<float>(0);
    if (input_spec.valid) {
      for (int i = 0; i < num_input_features; i++) {
        input[i] = (ml_input[i] - input_spec.mean[i]) * input_spec.invstd[i];
      }
    } else {
      for (int i = 0; i < num_input_features; i++) {
        input[i] = ml_input[i];
      }
    }
  } else if (model_version == 6) {
    uint8_t *input_order = &ctx->input_order[model_type][0];

    float *input0 = interpreter->typed_input_tensor<float>(input_order[0]);
    float *input1 = interpreter->typed_input_tensor<float>(input_order[1]);
    float *input2 = interpreter->typed_input_tensor<float>(input_order[2]);
    float *input3 = interpreter->typed_input_tensor<float>(input_order[3]);

    input0[0]  = norm(ml_input, FEATURE_INTER_RD_MULT, input_spec);
    input0[1]  = norm(ml_input, FEATURE_INTER_SWITCH, input_spec);
    input0[2]  = norm(ml_input, FEATURE_INTER_PART_T, input_spec);

    input1[0]  = norm(ml_input, FEATURE_INTER_FULL_PSNR, input_spec);
    input1[1]  = norm(ml_input, FEATURE_INTER_SQ_0_PSNR, input_spec);
    input1[2]  = norm(ml_input, FEATURE_INTER_SQ_1_PSNR, input_spec);
    input1[3]  = norm(ml_input, FEATURE_INTER_SQ_2_PSNR, input_spec);
    input1[4]  = norm(ml_input, FEATURE_INTER_SQ_3_PSNR, input_spec);
    input1[5]  = norm(ml_input, FEATURE_INTER_FULL_Q_COEFF_MAX, input_spec);
    input1[6]  = norm(ml_input, FEATURE_INTER_SQ_0_Q_COEFF_MAX, input_spec);
    input1[7]  = norm(ml_input, FEATURE_INTER_SQ_1_Q_COEFF_MAX, input_spec);
    input1[8]  = norm(ml_input, FEATURE_INTER_SQ_2_Q_COEFF_MAX, input_spec);
    input1[9]  = norm(ml_input, FEATURE_INTER_SQ_3_Q_COEFF_MAX, input_spec);
    input1[10] = norm(ml_input, FEATURE_INTER_FULL_Q_COEFF_NONZ, input_spec);
    input1[11] = norm(ml_input, FEATURE_INTER_SQ_0_Q_COEFF_NONZ, input_spec);
    input1[12] = norm(ml_input, FEATURE_INTER_SQ_1_Q_COEFF_NONZ, input_spec);
    input1[13] = norm(ml_input, FEATURE_INTER_SQ_2_Q_COEFF_NONZ, input_spec);
    input1[14] = norm(ml_input, FEATURE_INTER_SQ_3_Q_COEFF_NONZ, input_spec);
    input1[15] = norm(ml_input, FEATURE_INTER_FULL_LOG_SATDQ, input_spec);
    input1[16] = norm(ml_input, FEATURE_INTER_SQ_0_LOG_SATDQ, input_spec);
    input1[17] = norm(ml_input, FEATURE_INTER_SQ_1_LOG_SATDQ, input_spec);
    input1[18] = norm(ml_input, FEATURE_INTER_SQ_2_LOG_SATDQ, input_spec);
    input1[19] = norm(ml_input, FEATURE_INTER_SQ_3_LOG_SATDQ, input_spec);

    input2[0]  = norm(ml_input, FEATURE_INTER_FULL_PSNR, input_spec);
    input2[1]  = norm(ml_input, FEATURE_INTER_HOR_0_PSNR, input_spec);
    input2[2]  = norm(ml_input, FEATURE_INTER_HOR_1_PSNR, input_spec);
    input2[3]  = norm(ml_input, FEATURE_INTER_FULL_Q_COEFF_MAX, input_spec);
    input2[4]  = norm(ml_input, FEATURE_INTER_HOR_0_Q_COEFF_MAX, input_spec);
    input2[5]  = norm(ml_input, FEATURE_INTER_HOR_1_Q_COEFF_MAX, input_spec);
    input2[6]  = norm(ml_input, FEATURE_INTER_FULL_Q_COEFF_NONZ, input_spec);
    input2[7]  = norm(ml_input, FEATURE_INTER_HOR_0_Q_COEFF_NONZ, input_spec);
    input2[8]  = norm(ml_input, FEATURE_INTER_HOR_1_Q_COEFF_NONZ, input_spec);
    input2[9]  = norm(ml_input, FEATURE_INTER_FULL_LOG_SATDQ, input_spec);
    input2[10] = norm(ml_input, FEATURE_INTER_HOR_0_LOG_SATDQ, input_spec);
    input2[11] = norm(ml_input, FEATURE_INTER_HOR_1_LOG_SATDQ, input_spec);

    input3[0]  = norm(ml_input, FEATURE_INTER_FULL_PSNR, input_spec);
    input3[1]  = norm(ml_input, FEATURE_INTER_VER_0_PSNR, input_spec);
    input3[2]  = norm(ml_input, FEATURE_INTER_VER_1_PSNR, input_spec);
    input3[3]  = norm(ml_input, FEATURE_INTER_FULL_Q_COEFF_MAX, input_spec);
    input3[4]  = norm(ml_input, FEATURE_INTER_VER_0_Q_COEFF_MAX, input_spec);
    input3[5]  = norm(ml_input, FEATURE_INTER_VER_1_Q_COEFF_MAX, input_spec);
    input3[6]  = norm(ml_input, FEATURE_INTER_FULL_Q_COEFF_NONZ, input_spec);
    input3[7]  = norm(ml_input, FEATURE_INTER_VER_0_Q_COEFF_NONZ, input_spec);
    input3[8]  = norm(ml_input, FEATURE_INTER_VER_1_Q_COEFF_NONZ, input_spec);
    input3[9]  = norm(ml_input, FEATURE_INTER_FULL_LOG_SATDQ, input_spec);
    input3[10] = norm(ml_input, FEATURE_INTER_VER_0_LOG_SATDQ, input_spec);
    input3[11] = norm(ml_input, FEATURE_INTER_VER_1_LOG_SATDQ, input_spec);
  }

  FLOATING_POINT_DISABLE_EXCEPTIONS
  auto status = interpreter->Invoke();
  FLOATING_POINT_RESTORE_EXCEPTIONS

  if (status != kTfLiteOk) {
    reporter->Report("Failed at invoke");
    exit(1);
  }

  float *output = interpreter->typed_output_tensor<float>(0);
  for (int i = 0; i < output_len; i++) {
    ml_output[i] = output[i];
  }
  return 0;
}

extern "C" void av2_part_prune_tflite_close(void **context) {
  Context *ctx = (Context *)*context;
  if (ctx != nullptr) delete ctx;
  *context = nullptr;
}

extern "C" const char *get_model_name(MODEL_TYPE type) {
  if (type >= MODEL_COUNT) {
    return "NA";
  }
  return models[type].enum_name;
}

extern "C" int get_model_part_type(MODEL_TYPE type) {
  if (type >= MODEL_COUNT) {
    return PT_INVAL;
  }
  return models[type].part_type;
}

extern "C" int get_model_n_features(MODEL_TYPE type) {
  if (type >= MODEL_COUNT) {
    return 0;
  }
  return models[type].n_features;
}
