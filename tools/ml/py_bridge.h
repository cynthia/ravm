#ifndef TOOLS_ML_PY_BRIDGE_H_
#define TOOLS_ML_PY_BRIDGE_H_

#ifdef __cplusplus
extern "C" {
#endif
#include <stddef.h>

void* py_datafile_open();
void py_datafile_close(void** context, const char* filename);
void py_datafile_add_0d(void** context, const char* file, const char* var_name,
                        float* var_data);
void py_datafile_add_1d(void** context, const char* file, const char* var_name,
                        float* var_data, size_t shape0);
void py_datafile_add_2d(void** context, const char* file, const char* var_name,
                        float* var_data, size_t shape0, size_t shape1);
void py_datafile_fold(void** to, void** from);
int py_datafile_delete_old_files(const char* _filename, size_t* count);
#ifdef __cplusplus
}
#endif

#endif  // TOOLS_ML_PY_BRIDGE_H_
