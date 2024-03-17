#version 460
#extension GL_EXT_ray_tracing : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  payload.hit = false;
  payload.emission = vec3(1.0) * 100;
}
