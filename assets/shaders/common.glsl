#ifndef GLSL_COMMON
#define GLSL_COMMON

#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_scalar_block_layout : require

const float PI = 3.141592653589793f;
const float INVPI = 1.0f / 3.141592653589793f;
const float EPS = 0.001f;

float max3(in vec3 v) { return max(v.x, max(v.y, v.z)); }

mat3 orthogonalBasis(in vec3 w)
{
  vec3 a = abs(w.x) > 0.9 ? vec3(0,1,0) : vec3(1,0,0);
  vec3 v = normalize(cross(w, a));
  vec3 u = cross(w, v);
  return mat3(u, v, w);
}



#endif
