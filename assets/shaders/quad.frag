#version 460

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

layout (set=0, binding=0) uniform sampler2D test;

vec3 acesFilm(const vec3 x) {
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    return (x * (a * x + b)) / (x * (c * x + d ) + e);
}

vec3 tonemapFilmic(const vec3 color) {
	vec3 x = max(vec3(0.0), color - 0.004);
	return (x * (6.2 * x + 0.5)) / (x * (6.2 * x + 1.7) + 0.06);
}


void main() {
  const float GAMMA = 2.2;
  const float exposure = 2.0;

  vec4 accBuffer = texture(test, in_UV);
  vec3 color = accBuffer.rgb / accBuffer.a;
  color = pow(color, vec3(1.0/GAMMA));
  color = vec3(1.0) - exp(-color * exposure);

  color = acesFilm(color);

  out_Color = vec4(color, 1.0);

}

