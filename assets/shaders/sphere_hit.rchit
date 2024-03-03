#version 460
#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"

layout(location = 0)rayPayloadInEXT HitPayload payload;

hitAttributeEXT vec3 spherePoint;

void main() {
  payload.hit = true;

  const vec3 center = vec3(0);
  vec3 normal = normalize(spherePoint - center);

  bool inside = dot(normal, gl_ObjectRayDirectionEXT) > 0.0f;
  if (inside) {
    normal = -normal;
  }

  payload.t = gl_HitTEXT;
  payload.color = vec3(0.4, 0.4, 0.7);
  payload.emission = vec3(0);
  payload.world_normal = normal;
}
