#version 460
#extension GL_EXT_buffer_reference2 : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "types.glsl"
#include "common.glsl"

layout(shaderRecordEXT, scalar) buffer ShaderRecord
{
	VertexData v;
  IndexData  i;
  uint[128] geometry_to_index;
};


hitAttributeEXT vec2 attribs;

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  vec3 baryCoords = vec3(1.0f - attribs.x - attribs.y, attribs.x, attribs.y);
  uint index_offset = geometry_to_index[gl_GeometryIndexEXT];

  const Vertex v0 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 0]];
  const Vertex v1 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 1]];
  const Vertex v2 = v.vertices[i.indices[index_offset + gl_PrimitiveID * 3 + 2]];

  payload.hit = true;
  vec3 object_normal = normalize(v0.normal * baryCoords.x + v1.normal * baryCoords.y + v2.normal * baryCoords.z);
  payload.world_normal = normalize((gl_ObjectToWorldEXT * vec4(object_normal, 0.0)).xyz);
  payload.color = vec3(0.6);

  payload.emission = vec3(0.0);
  if (gl_GeometryIndexEXT == 4) {
    payload.emission = vec3(4.6, 0.0, 0.0);
  }

  payload.t = gl_HitTEXT;
}
