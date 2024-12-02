#version 460
#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;

layout(push_constant, std430) uniform Registers {
  PushConstants pc;
};


hitAttributeEXT vec3 spherePoint;

void main() {
  const Material material = pc.materials.materials[gl_InstanceCustomIndexEXT];

  // center in object space
  const vec3 center = vec3(0);

  // calculate object space normal and convert to world space
  vec3 surface_normal = mat3(gl_ObjectToWorldEXT) * normalize(spherePoint - center);

  const bool inside = dot(surface_normal, gl_WorldRayDirectionEXT) > 0.0f;
  if (inside) { surface_normal = -surface_normal; }
  const vec3 world_normal = surface_normal;

  payload.t = gl_HitTEXT;
  // purple-ish
  payload.absorption = vec3(0.3, 0.7, 0.3)*0;

  payload.color = material.base_color_factor;
  payload.emission = material.base_emissive_factor.rgb;
  const float roughness = material.roughness_factor;
  const float metallic = material.metallic_factor;
  const float transmission = material.specular_transmission_factor;

  payload.refract_index = material.refract_index;
  payload.surface_and_world_normal = pack2_normals(surface_normal, world_normal);
  hitPayloadSetTransmission(payload, transmission);
  hitPayloadSetRoughness(payload, roughness);
  hitPayloadSetMetallic(payload, metallic);
  hitPayloadSetInside(payload, inside);
}
