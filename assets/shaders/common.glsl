#ifndef GLSL_COMMON
#define GLSL_COMMON

#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_scalar_block_layout : require

const float PI = 3.141592653589793f;
const float INVPI = 1.0f / 3.141592653589793f;
const float EPS = 0.001f;

float max3(in vec3 v) { return max(v.x, max(v.y, v.z)); }

mat3 orthonormalBasis(in vec3 n)
{
  vec3 a;
  if (abs(n.x) > abs(n.z)) {
    a = vec3(-n.y, n.x, 0.0);
  } else {
    a = vec3(0.0, -n.z, n.y);
  }

  vec3 w = normalize(n);
  vec3 u = normalize(cross(a, w));
  vec3 v = cross(w, u);

  return mat3(u, v, w);
}

float saturate(float x)
{
  return max(0, min(1, x));
}

#endif
