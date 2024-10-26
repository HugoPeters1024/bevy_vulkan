#version 460

#include "types.glsl"

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

layout (set=0, binding=0) uniform sampler2D test;

layout(push_constant, std430) uniform Registers {
  UniformData uniforms;
};

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

vec3 applyVignette(vec3 color) {
    // Find the distance from the center of the screen
    vec2 uv = in_UV - 0.5;
    float dist = length(uv);

    // Parameters for controlling the vignette strength and smoothness
    float vignetteStrength = 0.35; // How much the vignette darkens (0.0 to 1.0)
    float vignetteRadius = 0.75;   // The radius from the center where the vignette starts

    // Calculate vignette factor based on the distance and radius
    float vignette = smoothstep(vignetteRadius, vignetteRadius - vignetteStrength, dist);

    // Apply the vignette effect by darkening the color at the corners
    return mix(color, color * vignette, vignetteStrength);
}


void main() {
  vec4 accBuffer = texture(test, in_UV);
  vec3 color = accBuffer.rgb / accBuffer.a;
  color = pow(color, vec3(1.0/uniforms.gamma));
  color = vec3(1.0) - exp(-color * uniforms.exposure);

  color = acesFilm(color);
  color = applyVignette(color);

  out_Color = vec4(color, 1.0);
}

