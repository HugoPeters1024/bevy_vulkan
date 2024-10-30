#version 460
#extension GL_EXT_buffer_reference2 : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"
#include "common.glsl"

layout(set=1, binding=200) uniform sampler2D textures[];

layout(shaderRecordEXT, scalar) buffer ShaderRecord
{
	VertexData v;
  TriangleData t;
  IndexData  i;
  GeometryData geometries;
  GeometryData triangles;
};

layout(push_constant, std430) uniform Registers {
  PushConstants pc;
};

layout(location = 0) rayPayloadInEXT HitPayload payload;

hitAttributeEXT vec2 attribs;

vec3 calcTangent(const Vertex v0, const Vertex v1, const Vertex v2) {
  const vec3 edge1 = v1.position - v0.position;
  const vec3 edge2 = v2.position - v0.position;
  const vec2 deltaUV1 = v1.texcoord - v0.texcoord;
  const vec2 deltaUV2 = v2.texcoord - v0.texcoord;


  const float denom = deltaUV1.x * deltaUV2.y - deltaUV2.x * deltaUV1.y;
  if (abs(denom) < 0.00001f) {
    return vec3(0.0, 0.0, 1.0);
  }

  vec3 tangent;
  const float f = 1.0 / denom;
  tangent.x = f * (deltaUV2.y * edge1.x - deltaUV1.y * edge2.x);
  tangent.y = f * (deltaUV2.y * edge1.y - deltaUV1.y * edge2.y);
  tangent.z = f * (deltaUV2.y * edge1.z - deltaUV1.y * edge2.z);

  return normalize(tangent);
}

vec4 toLinear(const vec4 sRGB)
{
  return pow(sRGB, vec4(2.2));
	const bvec4 cutoff = lessThan(sRGB, vec4(0.04045));
	const vec4 higher = pow((sRGB + vec4(0.055))/vec4(1.055), vec4(2.4));
	const vec4 lower = sRGB/vec4(12.92);

	return mix(higher, lower, cutoff);
}

#define PACKED 1

void main() {
  const vec3 baryCoords = vec3(1.0f - attribs.x - attribs.y, attribs.x, attribs.y);
  const Material material = pc.materials.materials[gl_InstanceCustomIndexEXT + gl_GeometryIndexEXT];

#if PACKED
  Triangle tri = t.triangles[triangles.index_offsets[gl_GeometryIndexEXT] + gl_PrimitiveID];
  const vec2 uv = mat3x2(
      unpackUv(tri.uvs[0]),
      unpackUv(tri.uvs[1]),
      unpackUv(tri.uvs[2])
  ) * baryCoords;
  vec3 object_normal = mat3(
      unpackNormal(tri.normals[0]),
      unpackNormal(tri.normals[1]),
      unpackNormal(tri.normals[2])
  ) * baryCoords;
  const vec3 tangent = unpackNormal(tri.tangent);
#else
  const uint index_offset = geometries.index_offsets[gl_GeometryIndexEXT];
  const Vertex v0 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 0]];
  const Vertex v1 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 1]];
  const Vertex v2 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 2]];
  const vec2 uv = v0.texcoord * baryCoords.x + v1.texcoord * baryCoords.y + v2.texcoord * baryCoords.z;
  vec3 object_normal = v0.normal * baryCoords.x + v1.normal * baryCoords.y + v2.normal * baryCoords.z;
  const vec3 tangent = calcTangent(v0, v1, v2);
#endif


  const bool inside = dot(object_normal, gl_ObjectRayDirectionEXT) > 0.0f;
  if (inside) { object_normal = -object_normal; }

  const vec3 surface_normal = normalize((gl_ObjectToWorldEXT * vec4(object_normal, 0.0)).xyz);
  payload.t = gl_HitTEXT;
  payload.refract_index = material.refract_index;
  payload.absorption = vec3(1.0);

  payload.color = material.base_color_factor * toLinear(texture(textures[material.base_color_texture], uv));
  payload.emission = material.base_emissive_factor.rgb * toLinear(texture(textures[material.base_emissive_texture], uv)).rgb;

  const float transmission = material.specular_transmission_factor * texture(textures[material.specular_transmission_texture], uv).r;

  const vec4 mr = texture(textures[material.metallic_roughness_texture], uv);
  const float roughness = material.roughness_factor * mr.g;
  const float metallic = material.metallic_factor * mr.b;

  const vec3 bitangent = cross(object_normal, tangent);
  const mat3 TBN = mat3(tangent, bitangent, object_normal);

  const vec3 texture_normal = texture(textures[material.normal_texture], uv).xyz * 2.0 - 1.0;
  const vec3 world_normal = normalize(mat3(gl_ObjectToWorldEXT) * TBN * texture_normal);

  payload.surface_and_world_normal = pack2_normals(surface_normal, world_normal);
  hitPayloadSetTransmission(payload, transmission);
  hitPayloadSetRoughness(payload, roughness);
  hitPayloadSetMetallic(payload, metallic);
  hitPayloadSetInside(payload, inside);
}
