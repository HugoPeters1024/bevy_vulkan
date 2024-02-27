#version 460
#extension GL_EXT_buffer_reference2 : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

hitAttributeEXT vec2 attribs;

struct HitPayload {
  bool hit;
};

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  if (gl_InstanceID == 0) {
    payload.hit = true;
  }
}
