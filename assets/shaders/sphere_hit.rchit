#version 460
#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;

layout(push_constant, std430) uniform Registers {
  UniformData uniforms;
  MaterialData materials;
};


hitAttributeEXT vec3 spherePoint;

void main() {
  const Material material = materials.materials[gl_InstanceID * 32];

  payload.hit = true;

  const vec3 center = vec3(0);
  vec3 normal = normalize(spherePoint - center);

  payload.inside = dot(normal, gl_ObjectRayDirectionEXT) > 0.0f;
  if (payload.inside) {
    normal = -normal;
  }

  payload.t = gl_HitTEXT;
  payload.color = vec3(0.4, 0.4, 0.7);
  payload.emission = vec3(0);
  payload.world_normal = normal;
  payload.roughness = 0.08;
  // purple-ish
  payload.absorption = vec3(0.3, 0.7, 0.3)*0;

  payload.color = material.base_color_factor.xyz;
  payload.emission = material.base_emissive_factor.rgb;
  payload.roughness = 0.0;
  payload.transmission = material.diffuse_transmission;
  payload.refract_index = 1.05;
}
