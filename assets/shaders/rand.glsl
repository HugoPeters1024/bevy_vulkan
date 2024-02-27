#ifndef GLSL_RAND
#define GLSL_RAND

uint g_seed = 666;

uint rand_xorshift(in uint seed)
{
    seed ^= (seed << 13);
    seed ^= (seed >> 17);
    seed ^= (seed << 5);
    return seed;
}

uint wang_hash(in uint seed)
{
    seed = (seed ^ 61) ^ (seed >> 16);
    seed *= 9;
    seed = seed ^ (seed >> 4);
    seed *= 0x27d4eb2d;
    seed = seed ^ (seed >> 15);
    return seed;
}

uint rand()
{
  uint prev = g_seed * 747796405u + 2891336453u;
  uint word = ((prev >> ((prev >> 28u) + 4u)) ^ prev) * 277803737u;
  g_seed     = prev;
  return (word >> 22u) ^ word;
}


float randf()
{
    uint r = rand();
    return uintBitsToFloat(0x3f800000 | (r >> 9)) - 1.0f;
}

vec3 CosineSampleHemisphere(float r1, float r2)
{
  const float TWO_PI = 6.28318530718;
  vec3  dir;
  float r   = sqrt(r1);
  float phi = TWO_PI * r2;
  dir.x     = r * cos(phi);
  dir.y     = r * sin(phi);
  dir.z     = sqrt(max(0.0, 1.0 - dir.x * dir.x - dir.y * dir.y));

  return dir;
}

vec3 hsv2rgb(vec3 c)
{
    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

vec3 SampleRandomColor()
{
  float h = randf();
  float s = 0.5 + 0.5 * randf();
  float v = 0.5 + 0.5 * randf();
  return hsv2rgb(vec3(h, s, v));
}

// NVIDIA 2021 nvpro-samples
uint tea(in uint val0, in uint val1)
{
  uint v0 = val0;
  uint v1 = val1;
  uint s0 = 0;

  for(uint n = 0; n < 16; n++)
  {
    s0 += 0x9e3779b9;
    v0 += ((v1 << 4) + 0xa341316c) ^ (v1 + s0) ^ ((v1 >> 5) + 0xc8013ea4);
    v1 += ((v0 << 4) + 0xad90777d) ^ (v0 + s0) ^ ((v0 >> 5) + 0x7e95761e);
  }

  return v0;
}

uint initRandom(in uvec2 resolution, in uvec2 screenCoord, in uint frame)
{
  return tea(screenCoord.y * resolution.x + screenCoord.x, frame);
}

#endif // GLSL_RAND

