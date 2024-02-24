#version 460

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

layout (set=0, binding=0) uniform sampler2D test;

void main() {
  out_Color = texture(test, in_UV);
}

