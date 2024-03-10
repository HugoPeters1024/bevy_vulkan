#version 460
#extension GL_EXT_buffer_reference2 : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"
#include "common.glsl"

layout(set=1, binding=42)         uniform sampler2D textures[];

layout(shaderRecordEXT, scalar) buffer ShaderRecord
{
	VertexData v;
  IndexData  i;
  uint[32] geometry_to_index;
};

layout(push_constant, std430) uniform Registers {
  UniformData uniforms;
  MaterialData materials;
};


hitAttributeEXT vec2 attribs;

layout(location = 0) rayPayloadInEXT HitPayload payload;

vec3 calcTangent(in Vertex v0, in Vertex v1, in Vertex v2) {
  vec3 edge1 = v1.position - v0.position;
  vec3 edge2 = v2.position - v0.position;
  vec2 deltaUV1 = v1.texcoord - v0.texcoord;
  vec2 deltaUV2 = v2.texcoord - v0.texcoord;


  float denom = deltaUV1.x * deltaUV2.y - deltaUV2.x * deltaUV1.y;
  if (abs(denom) < 0.0001f) {
    return vec3(1.0, 0.0, 0.0);
  }

  vec3 tangent;
  float f = 1.0 / denom;
  tangent.x = f * (deltaUV2.y * edge1.x - deltaUV1.y * edge2.x);
  tangent.y = f * (deltaUV2.y * edge1.y - deltaUV1.y * edge2.y);
  tangent.z = f * (deltaUV2.y * edge1.z - deltaUV1.y * edge2.z);
  return normalize(tangent);
}


void main() {
  vec3 baryCoords = vec3(1.0f - attribs.x - attribs.y, attribs.x, attribs.y);
  uint index_offset = geometry_to_index[gl_GeometryIndexEXT];

  const Material material = materials.materials[gl_InstanceID * 32 + gl_GeometryIndexEXT];
  const Vertex v0 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 0]];
  const Vertex v1 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 1]];
  const Vertex v2 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 2]];

  vec2 uv = v0.texcoord * baryCoords.x + v1.texcoord * baryCoords.y + v2.texcoord * baryCoords.z;

  payload.hit = true;
  vec3 object_normal = normalize(v0.normal * baryCoords.x + v1.normal * baryCoords.y + v2.normal * baryCoords.z);

  payload.inside = dot(object_normal, gl_ObjectRayDirectionEXT) > 0.0f;

  if (payload.inside) {
    object_normal = -object_normal;
  }

  payload.surface_normal = normalize((gl_ObjectToWorldEXT * vec4(object_normal, 0.0)).xyz);
  payload.t = gl_HitTEXT;
  payload.refract_index = 1.0;
  payload.absorption = vec3(0.0);
  payload.roughness = material.roughness_factor;

  payload.color = material.base_color_factor.xyz;
  if (material.base_color_texture != 0xFFFFFFFF) {
    payload.color *= pow(texture(textures[material.base_color_texture], uv).xyz, vec3(2.2));
  }

  payload.emission = material.base_emissive_factor.rgb;
  if (material.base_emissive_texture != 0xFFFFFFFF) {
    payload.emission *= texture(textures[material.base_emissive_texture], uv).xyz;
  }

  payload.transmission = material.specular_transmission_factor;
  if (material.specular_transmission_texture != 0xFFFFFFFF) {
    payload.transmission *= texture(textures[material.specular_transmission_texture], uv).x;
  }

  if (material.normal_texture != 0xFFFFFFFF) {
    const vec3 tangent = calcTangent(v0, v1, v2);
    const vec3 bitangent = cross(object_normal, tangent);
    const mat3 TBN = mat3(tangent, bitangent, object_normal);

    const vec3 texture_normal = texture(textures[material.normal_texture], uv).xyz * 2.0 - 1.0;
    payload.world_normal = normalize(TBN * texture_normal);
  } else {
    payload.world_normal = payload.surface_normal;
  }

  if (payload.transmission > 0.0) {
    payload.roughness = 0.0;
  }
}
