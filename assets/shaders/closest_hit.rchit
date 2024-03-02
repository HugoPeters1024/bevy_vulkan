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
};


hitAttributeEXT vec2 attribs;

layout(location = 0) rayPayloadInEXT HitPayload payload;

void main() {
  vec3 baryCoords = vec3(1.0f - attribs.x - attribs.y, attribs.x, attribs.y);

  const Vertex v0 = v.vertices[i.indices[gl_PrimitiveID * 3 + 0]];
  const Vertex v1 = v.vertices[i.indices[gl_PrimitiveID * 3 + 1]];
  const Vertex v2 = v.vertices[i.indices[gl_PrimitiveID * 3 + 2]];

  payload.hit = true;
  vec3 object_normal = v0.normal * baryCoords.x + v1.normal * baryCoords.y + v2.normal * baryCoords.z;
  payload.world_normal = normalize((gl_ObjectToWorldEXT * vec4(object_normal, 0.0)).xyz);
  payload.color = abs(payload.world_normal);
  payload.t = gl_HitTEXT;
}
