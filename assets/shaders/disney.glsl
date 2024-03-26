#ifndef H_DISNEY
#define H_DISNEY

#include "rand.glsl"

// From: https://www.shadertoy.com/view/cll3R4

struct DisneyMaterial {
    vec3 albedo;
    float metallic;
    float roughness;
    vec3 emissive;
};

// ---------------------------------------------
// Microfacet
// ---------------------------------------------
vec3 F_Schlick(vec3 f0, float theta) {
    return f0 + (1.-f0) * pow(1.0-theta, 5.);
}

float F_Schlick(float f0, float f90, float theta) {
    return f0 + (f90 - f0) * pow(1.0-theta, 5.0);
}

float D_GTR(float roughness, float NoH, float k) {
    float a2 = pow(roughness, 2.);
    return a2 / (PI * pow((NoH*NoH)*(a2*a2-1.)+1., k));
}

float SmithG(float NDotV, float alphaG)
{
    float a = alphaG * alphaG;
    float b = NDotV * NDotV;
    return (2.0 * NDotV) / (NDotV + sqrt(a + b - a * b));
}

float GeometryTerm(float NoL, float NoV, float roughness)
{
    float a2 = roughness*roughness;
    float G1 = SmithG(NoV, a2);
    float G2 = SmithG(NoL, a2);
    return G1*G2;
}

vec3 SampleGGXVNDF(vec3 V, float ax, float ay, float r1, float r2)
{
    vec3 Vh = normalize(vec3(ax * V.x, ay * V.y, V.z));

    float lensq = Vh.x * Vh.x + Vh.y * Vh.y;
    vec3 T1 = lensq > 0. ? vec3(-Vh.y, Vh.x, 0) * inversesqrt(lensq) : vec3(1, 0, 0);
    vec3 T2 = cross(Vh, T1);

    float r = sqrt(r1);
    float phi = 2.0 * PI * r2;
    float t1 = r * cos(phi);
    float t2 = r * sin(phi);
    float s = 0.5 * (1.0 + Vh.z);
    t2 = (1.0 - s) * sqrt(1.0 - t1 * t1) + s * t2;

    vec3 Nh = t1 * T1 + t2 * T2 + sqrt(max(0.0, 1.0 - t1 * t1 - t2 * t2)) * Vh;

    return normalize(vec3(ax * Nh.x, ay * Nh.y, max(0.0, Nh.z)));
}

float GGXVNDFPdf(float NoH, float NoV, float roughness)
{
 	float D = D_GTR(roughness, NoH, 2.);
    float G1 = SmithG(NoV, roughness*roughness);
    return (D * G1) / max(0.00001, 4.0f * NoV);
}

// ---------------------------------------------
// Maths
// ---------------------------------------------
#define saturate(x) clamp(x,0.,1.)
#define PI 3.141592653589

mat3 lookat(vec3 ro, vec3 ta)
{
    const vec3 up = vec3(0.,1.,0.);
    vec3 fw = normalize(ta-ro);
	vec3 rt = normalize( cross(fw, normalize(up)) );
	return mat3( rt, cross(rt, fw), fw );
}

mat2 rot(float v) {
    float a = cos(v);
    float b = sin(v);
    return mat2(a,b,-b,a);
}

// From fizzer - https://web.archive.org/web/20170610002747/http://amietia.com/lambertnotangent.html
vec3 cosineSampleHemisphere(vec3 n, float r1, float r2)
{
    float a = PI*2.*r1;
    float b = 2.0*r2-1.0;

    vec3 dir = vec3(sqrt(1.0-b*b)*vec2(cos(a),sin(a)),b);
    return normalize(n + dir);
}

// From pixar - https://graphics.pixar.com/library/OrthonormalB/paper.pdf
void basis(in vec3 n, out vec3 b1, out vec3 b2)
{
    if(n.z<0.){
        float a = 1.0 / (1.0 - n.z);
        float b = n.x * n.y * a;
        b1 = vec3(1.0 - n.x * n.x * a, -b, n.x);
        b2 = vec3(b, n.y * n.y*a - 1.0, -n.y);
    }
    else{
        float a = 1.0 / (1.0 + n.z);
        float b = -n.x * n.y * a;
        b1 = vec3(1.0 - n.x * n.x * a, b, -n.x);
        b2 = vec3(b, 1.0 - n.y * n.y * a, -n.y);
    }
}

vec3 toWorld(vec3 x, vec3 y, vec3 z, vec3 v)
{
    return v.x*x + v.y*y + v.z*z;
}

vec3 toLocal(vec3 x, vec3 y, vec3 z, vec3 v)
{
    return vec3(dot(v, x), dot(v, y), dot(v, z));
}

// ---------------------------------------------
// Color
// ---------------------------------------------
vec3 RGBToYCoCg(vec3 rgb)
{
	float y  = dot(rgb, vec3(  1, 2,  1 )) * 0.25;
	float co = dot(rgb, vec3(  2, 0, -2 )) * 0.25 + ( 0.5 * 256.0/255.0 );
	float cg = dot(rgb, vec3( -1, 2, -1 )) * 0.25 + ( 0.5 * 256.0/255.0 );
	return vec3(y, co, cg);
}

vec3 YCoCgToRGB(vec3 ycocg)
{
	float y = ycocg.x;
	float co = ycocg.y - ( 0.5 * 256.0 / 255.0 );
	float cg = ycocg.z - ( 0.5 * 256.0 / 255.0 );
	return vec3(y + co-cg, y + cg, y - co-cg);
}

float luma(vec3 color) {
    return dot(color, vec3(0.299, 0.587, 0.114));
}

// ---------------------------------------------
// BRDF
// ---------------------------------------------
vec3 evalDisneyDiffuse(DisneyMaterial mat, float NoL, float NoV, float LoH, float roughness) {
    float FD90 = 0.5 + 2. * roughness * pow(LoH,2.);
    float a = F_Schlick(1.,FD90, NoL);
    float b = F_Schlick(1.,FD90, NoV);

    return mat.albedo * (a * b / PI);
}

vec3 evalDisneySpecular(DisneyMaterial mat, vec3 F, float NoH, float NoV, float NoL) {
    float roughness = pow(mat.roughness, 2.);
    float D = D_GTR(roughness, NoH,2.);
    float G = GeometryTerm(NoL, NoV, pow(0.5+mat.roughness*.5,2.));

    vec3 spec = D*F*G / (4. * NoL * NoV);

    return spec;
}

vec4 sampleDisneyBRDF(vec4 noise, vec3 v, vec3 n, DisneyMaterial mat, inout vec3 l) {

    float roughness = pow(mat.roughness, 2.);

    // sample microfacet normal
    vec3 t,b;
    basis(n,t,b);
    vec3 V = toLocal(t,b,n,v);
    vec3 h = SampleGGXVNDF(V, roughness,roughness, noise.z, noise.w);
    if (h.z < 0.0)
        h = -h;
    h = toWorld(t,b,n,h);

    // fresnel
    vec3 f0 = mix(vec3(0.04), mat.albedo, mat.metallic);
    vec3 F = F_Schlick(f0, dot(v,h));

    // lobe weight probability
    float diffW = (1.-mat.metallic);
    float specW = luma(F);
    float invW = 1./(diffW + specW);
    diffW *= invW;
    specW *= invW;


    vec4 brdf = vec4(0.);
    float rnd = randf();
    if (rnd < diffW) // diffuse
    {
        l = cosineSampleHemisphere(n, noise.x, noise.y);
        h = normalize(l+v);

        float NoL = dot(n,l);
        float NoV = dot(n,v);
        if ( NoL <= 0. || NoV <= 0. ) { return vec4(0.); }
        float LoH = dot(l,h);
        float pdf = NoL/PI;

        vec3 diff = evalDisneyDiffuse(mat, NoL, NoV, LoH, roughness) * (1.-F);
        brdf.rgb = diff * NoL;
        brdf.a = diffW * pdf;
    }
    else // specular
    {
        l = reflect(-v,h);

        float NoL = dot(n,l);
        float NoV = dot(n,v);
        if ( NoL <= 0. || NoV <= 0. ) { return vec4(0.); }
        float NoH = min(dot(n,h),.99);
        float pdf = GGXVNDFPdf(NoH, NoV, roughness);

        vec3 spec = evalDisneySpecular(mat, F, NoH, NoV, NoL);
        brdf.rgb = spec * NoL;
        brdf.a = specW * pdf;
    }

    return brdf;
}







#endif // H_DISNEY

