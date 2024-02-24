#version 460
#extension GL_EXT_ray_tracing : enable

struct HitPayload {
  float t;
};

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  payload.t = 10000000.0;
}
