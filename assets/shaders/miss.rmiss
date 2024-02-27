#version 460
#extension GL_EXT_ray_tracing : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  // sky blue
  payload.t = 0.0;
  payload.emission = vec3(0.0, 0.16, 0.4);
}
