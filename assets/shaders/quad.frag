#version 460

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

void main() {
  out_Color = vec4(in_UV, 0.0f, 1.0);
}

