#version 460
#extension GL_EXT_buffer_reference : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"

layout(location = 0) rayPayloadInEXT HitPayload payload;

layout(push_constant, std430) uniform Registers {
  UniformData uniforms;
  MaterialData materials;
  BluenoiseData bluenoise;
  FocusData focus;
  uint skydome;
  uint _padding;
};


hitAttributeEXT vec3 spherePoint;

void main() {
  const Material material = materials.materials[gl_InstanceCustomIndexEXT];

  const vec3 center = vec3(0);
  payload.surface_normal = normalize(spherePoint - center);

  payload.inside = dot(payload.surface_normal, gl_ObjectRayDirectionEXT) > 0.0f;
  if (payload.inside) {
    payload.surface_normal = -payload.surface_normal;
  }

  payload.t = gl_HitTEXT;
  payload.world_normal = mat3(gl_ObjectToWorldEXT) * payload.surface_normal;
  // purple-ish
  payload.absorption = vec3(0.3, 0.7, 0.3)*0;

  payload.color = material.base_color_factor.xyz;
  payload.emission = material.base_emissive_factor.rgb;
  payload.roughness = material.roughness_factor;
  payload.roughness = 0.0;
  payload.metallic = material.metallic_factor;
  payload.transmission = material.specular_transmission_factor;
  payload.refract_index = 1.15;
}
