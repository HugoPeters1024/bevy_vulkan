#version 460

layout(location = 0) in  vec2 in_UV;
layout(location = 0) out vec4 out_Color;

layout (set=0, binding=0) uniform sampler2D test;
layout (set=0, binding=1) uniform sampler2D diff;
layout (set=0, binding=2) uniform sampler2D albedo;

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

vec3 _NRD_YCoCgToLinear( vec3 color )
{
    float t = color.x - color.z;

    vec3 r;
    r.y = color.x + color.z;
    r.x = t + color.y;
    r.z = t - color.y;

    return max( r, 0.0 );
}


vec4 REBLUR_BackEnd_UnpackRadianceAndNormHitDist( vec4 data )
{
    data.xyz = _NRD_YCoCgToLinear( data.xyz );

    return data;
}



void main() {
  const float GAMMA = 2.2;
  const float exposure = 1.0;

  vec4 accBuffer = texture(test, in_UV);
  if (in_UV.x > 0.2 && accBuffer.a < 128) {
    vec4 diffColor = texture(diff, in_UV);
    accBuffer = vec4(REBLUR_BackEnd_UnpackRadianceAndNormHitDist(diffColor).rgb, 1.0);

    vec4 albedoColor = texture(albedo, in_UV);
    accBuffer.rgb *= albedoColor.rgb;
  }

  vec3 color = accBuffer.rgb / accBuffer.a;
  color = pow(color, vec3(1.0/GAMMA));
  color = vec3(1.0) - exp(-color * exposure);

  color = acesFilm(color);

  out_Color = vec4(color, 1.0);

}

