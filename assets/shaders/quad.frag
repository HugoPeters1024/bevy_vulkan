#version 460

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

layout (set=0, binding=0) uniform sampler2D test;

void main() {
  const float GAMMA = 2.2;
  out_Color = pow(texture(test, in_UV), vec4(1.0/GAMMA));
}

