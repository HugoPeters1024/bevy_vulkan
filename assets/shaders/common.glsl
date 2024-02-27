#ifndef GLSL_COMMON
#define GLSL_COMMON

#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_scalar_block_layout : require

const float PI = 3.141592653589793f;
const float INVPI = 1.0f / 3.141592653589793f;
const float EPS = 0.001f;

float max3(in vec3 v) { return max(v.x, max(v.y, v.z)); }


#endif
