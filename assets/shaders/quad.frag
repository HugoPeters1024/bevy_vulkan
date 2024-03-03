#version 460

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

layout (set=0, binding=0) uniform sampler2D test;

void main() {
  const float GAMMA = 2.2;

  vec4 accBuffer = texture(test, in_UV);
  vec3 color = accBuffer.rgb / accBuffer.a;
  color = pow(color, vec3(1.0/GAMMA));
  out_Color = vec4(color, 1.0);
}

