#include "Geometry.h"
#include <math/mat3.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <map>
#include <tuple>

namespace forest {
namespace {
constexpr float pi = 3.14159265359f;
struct Random {
    uint32_t state = 0x6f726573;
    float next(float lo, float hi) {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        return lo + (hi - lo) * float(state >> 8) / float(0x1000000);
    }
};
struct Builder {
    Random random;
    std::map<std::tuple<int, int, Surface>, Mesh> batches;
    Mesh& mesh(float3 p, Surface s) {
        auto key = std::make_tuple(int(std::floor(p.x / 10)), int(std::floor(p.z / 10)), s);
        return batches.try_emplace(key, Mesh{s, {}, {}}).first->second;
    }
    void triangle(Mesh& m, float3 a, float3 b, float3 c,
            float2 ua, float2 ub, float2 uc, float4 color) {
        const float3 n = normalize(cross(b - a, c - a));
        const float2 du = ub - ua, dv = uc - ua;
        const float det = du.x * dv.y - du.y * dv.x;
        float3 t = std::abs(det) > 1e-7f ? ((b-a)*dv.y - (c-a)*du.y)/det : b-a;
        t = normalize(t - n * dot(n, t));
        float3 bitangent = cross(n, t) * (det < 0 ? -1.f : 1.f);
        const quatf q = mat3f::packTangentFrame(mat3f{t, bitangent, n});
        const auto base = uint32_t(m.vertices.size());
        m.vertices.insert(m.vertices.end(), {{a,q,ua,color},{b,q,ub,color},{c,q,uc,color}});
        m.indices.insert(m.indices.end(), {base,base+1,base+2});
    }
    void quad(Mesh& m, float3 a, float3 b, float3 c, float3 d, float4 color,
              float u = 1, float v = 1) {
        triangle(m,a,b,c,{0,0},{u,0},{u,v},color);
        triangle(m,a,c,d,{0,0},{u,v},{0,v},color);
    }
    // Beveled, slightly skewed stone: six inset faces, twelve bevels, eight corners.
    void stone(float3 p, float3 half, float yaw = 0) {
        Mesh& m = mesh(p, Surface::Stone);
        float bevel = std::min({half.x,half.y,half.z}) * random.next(.12f,.23f);
        float skew = random.next(-.06f,.06f);
        auto world = [&](float3 a) {
            a.x += skew * a.y;
            return p + float3{a.x*std::cos(yaw)+a.z*std::sin(yaw),a.y,
                              -a.x*std::sin(yaw)+a.z*std::cos(yaw)};
        };
        const float shade = random.next(.72f,1.10f);
        const float4 color{shade,shade,shade*.96f,1};
        auto face = [&](std::vector<float3> vs) {
            float3 n = cross(vs[1]-vs[0],vs[2]-vs[0]);
            float3 center{}; for (auto v:vs) center += v;
            if (dot(n,center)<0) std::reverse(vs.begin(),vs.end());
            n = normalize(cross(vs[1]-vs[0],vs[2]-vs[0]));
            int axis = std::abs(n.x)>.7f ? 0 : (std::abs(n.y)>.7f ? 1 : 2);
            auto uv = [&](float3 v) { return axis==0 ? float2{v.z,v.y}/2.f :
                (axis==1 ? float2{v.x,v.z}/2.f : float2{v.x,v.y}/2.f); };
            const float2 offset{p.x*.37f,p.z*.41f};
            for (size_t i=1;i+1<vs.size();++i)
                triangle(m,world(vs[0]),world(vs[i]),world(vs[i+1]),
                    uv(vs[0])+offset,uv(vs[i])+offset,uv(vs[i+1])+offset,color);
        };
        for (int axis=0;axis<3;++axis) for (float sign:{-1.f,1.f}) {
            int u=(axis+1)%3,v=(axis+2)%3;
            std::vector<float3> vs;
            for (auto s:std::array<float2,4>{{{-1,-1},{1,-1},{1,1},{-1,1}}}) {
                float3 a{}; a[axis]=sign*half[axis];
                a[u]=s.x*(half[u]-bevel); a[v]=s.y*(half[v]-bevel); vs.push_back(a);
            } face(vs);
        }
        for (int axis=0;axis<3;++axis) for(float s:{-1.f,1.f}) for(float t:{-1.f,1.f}) {
            int u=(axis+1)%3,v=(axis+2)%3;
            std::vector<float3> vs;
            for (int j=0;j<4;++j) {
                float3 a{}; a[axis]=(j<2?-1.f:1.f)*(half[axis]-bevel);
                bool first=(j==0||j==3);
                a[u]=s*(half[u]-(first?0:bevel)); a[v]=t*(half[v]-(first?bevel:0));
                vs.push_back(a);
            } face(vs);
        }
        for(float x:{-1.f,1.f}) for(float y:{-1.f,1.f}) for(float z:{-1.f,1.f}) {
            float3 s{x,y,z}; std::vector<float3> vs;
            for(int a=0;a<3;++a) { float3 v=s*(half-float3{bevel});v[a]=s[a]*half[a];vs.push_back(v); }
            face(vs);
        }
    }
    void branch(float3 a, float3 b, float r0, float r1, int sides=9) {
        Mesh& m=mesh(a,Surface::Bark);
        float3 n=normalize(b-a),t=normalize(cross(n,std::abs(n.y)<.9f?float3{0,1,0}:float3{1,0,0}));
        float3 v=cross(n,t);
        float len=length(b-a);
        for(int i=0;i<sides;++i) {
            float s=2*pi*i/sides,e=2*pi*(i+1)/sides;
            float3 d=t*std::cos(s)+v*std::sin(s), f=t*std::cos(e)+v*std::sin(e);
            quad(m,a+d*r0,a+f*r0,b+f*r1,b+d*r1,{.88f,.88f,.83f,1},
                 2*pi*r0/sides,len);
        }
    }
    void leaf(float3 p, float3 direction, float width, float4 color) {
        Mesh& m=mesh(p,Surface::Leaves);
        float3 side=normalize(cross(direction,float3{0,1,.15f}))*width;
        float3 mid=p+direction*.45f;
        float3 ridge=mid+float3{0,width*.16f,0};
        triangle(m,p,mid+side,ridge,{.5f,0},{1,.45f},{.5f,.45f},color);
        triangle(m,mid+side,p+direction,ridge,{1,.45f},{.5f,1},{.5f,.45f},color);
        triangle(m,p+direction,mid-side,ridge,{.5f,1},{0,.45f},{.5f,.45f},color);
        triangle(m,mid-side,p,ridge,{0,.45f},{.5f,0},{.5f,.45f},color);
    }
    void sprig(float3 p,float3 direction,float width) {
        Mesh& m=mesh(p,Surface::Canopy);
        float3 side=normalize(cross(direction,float3{0,1,.15f}))*width;
        float3 up=normalize(cross(direction,side))*width;
        for(auto span:{side,up})
            quad(m,p-span,p+span,p+direction+span,p+direction-span,{1,1,1,1});
    }
    void tree(float x,float z,float h) {
        float3 root{x,elevation(x,z)-.06f,z}, prev=root;
        float radius=h*random.next(.025f,.039f);
        float3 lean{random.next(-1.3f,1.3f),0,random.next(-1.3f,1.3f)};
        for(int j=0;j<6;++j) {
            float f=(j+1)/6.f;
            float3 next=root+float3{0,h*f,0}+lean*f*f;
            branch(prev,next,radius*(1-j/6.f)*.9f+.025f,radius*(1-f)*.9f+.025f,12);
            prev=next;
        }
        for(int j=0;j<5;++j) {
            float a=j*2*pi/5+random.next(-.3f,.3f);
            float3 end=root+float3{std::cos(a)*radius*4,0,std::sin(a)*radius*4};
            end.y=elevation(end.x,end.z)+.03f;
            branch(root+float3{0,.45f,0},end,radius*.45f,.035f);
        }
        for(int j=0;j<9;++j) {
            float a=j*2.39996f+random.next(-.3f,.3f);
            float f=.45f+.045f*j;
            float3 base=root+float3{0,h*f,0}+lean*f*f;
            float spread=h*(1.15f-f)*random.next(.5f,.8f);
            float3 end=base+float3{std::cos(a)*spread,h*.12f,std::sin(a)*spread};
            branch(base,end,radius*(1-f)*.7f,.025f,7);
            for(int k=0;k<3;++k) {
                float b=a+(k%2?1:-1)*.8f;
                float3 twig=base+(end-base)*(.35f+k*.2f);
                float3 tip=twig+float3{std::cos(b),.3f,std::sin(b)}*spread*.48f;
                branch(twig,tip,.035f,.007f,5);
                for(int l=0;l<10;++l) {
                    float3 p=twig+(tip-twig)*random.next(0,1)+float3{
                        random.next(-.65f,.65f),random.next(-.25f,.5f),random.next(-.65f,.65f)};
                    float angle=random.next(0,2*pi),size=random.next(.65f,1.15f);
                    sprig(p,float3{std::cos(angle)*size,random.next(-.15f,.3f),std::sin(angle)*size},size*.24f);
                }
            }
        }
    }
    void fern(float x,float z,float scale) {
        float3 root{x,elevation(x,z),z};
        for(int f=0;f<6;++f) {
            float a=f*2*pi/6+random.next(-.18f,.18f);
            float3 axis{std::cos(a),0,std::sin(a)},side{-axis.z,0,axis.x};
            for(int j=1;j<8;++j) {
                float t=j/8.f;
                float3 p=root+axis*t*scale+float3{0,std::sin(t*pi*.85f)*scale*.65f,0};
                for(float sign:{-1.f,1.f}) {
                    float3 d=(side*sign*.25f*(1-t)+axis*.12f)*scale;
                    leaf(p,d,.045f*scale*(1-t),{.036f,.135f,.028f,.35f});
                }
            }
        }
    }
    void rock(float3 p, float3 size) {
        Mesh& m=mesh(p,Surface::Stone);
        constexpr int sides=11, rings=6;
        std::array<std::array<float3,sides+1>,rings+1> points{};
        for(int y=0;y<=rings;++y) for(int x=0;x<sides;++x) {
            float theta=pi*y/rings,phi=2*pi*x/sides;
            float radius=random.next(.88f,1.12f);
            points[y][x]=p+float3{std::sin(theta)*std::cos(phi),std::cos(theta),
                std::sin(theta)*std::sin(phi)}*size*radius;
            if(y==0 || y==rings) points[y][x]=p+float3{0,y==0?size.y:-size.y,0};
        }
        for(int y=0;y<=rings;++y)points[y][sides]=points[y][0];
        // Outward CCW winding; triangle() derives the normal and tangent frame.
        for(int y=0;y<rings;++y)for(int x=0;x<sides;++x) {
            auto a=points[y][x],b=points[y][x+1],c=points[y+1][x+1],d=points[y+1][x];
            auto uv=[&](float3 v){return float2{v.x+v.y*.3f,v.z+v.y*.4f}/2.f;};
            if(y!=0)triangle(m,a,b,c,uv(a),uv(b),uv(c),{.85f,.88f,.8f,1});
            if(y!=rings-1)triangle(m,a,c,d,uv(a),uv(c),uv(d),{.85f,.88f,.8f,1});
        }
    }
    void arch(float z, float scale) {
        for(float side:{-1.f,1.f}) for(int j=0;j<5;++j)
            stone({side*1.85f*scale,(j+.5f)*.55f*scale,z},{.43f*scale,.265f*scale,.55f*scale});
        Mesh& m=mesh({0,0,z},Surface::Stone);
        for(int j=0;j<13;++j) {
            float a=j*pi/13+.012f,b=(j+1)*pi/13-.012f;
            float shade=random.next(.75f,1.05f);
            float4 color{shade,shade,shade*.96f,1};
            auto p=[&](float angle,float r,float depth){return float3{
                std::cos(angle)*r*scale,(2.75f+std::sin(angle)*r)*scale,z+depth*scale};};
            float3 ai=p(a,1.43f,.55f),ao=p(a,2.28f,.55f),bi=p(b,1.43f,.55f),bo=p(b,2.28f,.55f);
            float3 aj=p(a,1.43f,-.55f),ak=p(a,2.28f,-.55f),bj=p(b,1.43f,-.55f),bk=p(b,2.28f,-.55f);
            quad(m,ai,ao,bo,bi,color,.45f,.45f);quad(m,ak,aj,bj,bk,color,.45f,.45f);
            quad(m,ao,ak,bk,bo,color,.5f,.45f);quad(m,aj,ai,bi,bj,color,.5f,.45f);
            quad(m,aj,ak,ao,ai,color,.45f,.5f);quad(m,bi,bo,bk,bj,color,.45f,.5f);
        }
    }
};
}
float elevation(float x,float z) {
    float hills=.36f*std::sin(x*.24f)*std::cos(z*.18f)+.12f*std::sin(x*.7f+z*.35f);
    // Flat ruin footprint blends into the forest floor.
    float blend=std::clamp((std::max(std::abs(x)-6.f,std::abs(z+3.f)-7.f))/5.f,0.f,1.f);
    return -.12f+hills*blend;
}
std::vector<Mesh> generate() {
    Builder b;
    for(int x=-40;x<40;++x) for(int z=-40;z<40;++z) {
        Mesh& m=b.mesh({float(x),0,float(z)},Surface::Ground);
        float3 a{float(x),elevation(x,z),float(z)},c{float(x+1),elevation(x+1,z+1),float(z+1)};
        float3 d{float(x),elevation(x,z+1),float(z+1)},e{float(x+1),elevation(x+1,z),float(z)};
        float4 color{.83f,.87f,.76f,1};
        b.triangle(m,a,d,c,{x/3.f,z/3.f},{x/3.f,(z+1)/3.f},{(x+1)/3.f,(z+1)/3.f},color);
        b.triangle(m,a,c,e,{x/3.f,z/3.f},{(x+1)/3.f,(z+1)/3.f},{(x+1)/3.f,z/3.f},color);
    }
    b.arch(-2,1);b.arch(-8,.86f);
    for(float side:{-1.f,1.f}) {
        for(int z=0;z<11;++z) {
            int height=2+int(b.random.next(0,4));
            for(int row=0;row<height;++row)
                b.stone({side*(3.6f+(row%2)*.04f),row*.5f+.13f,-float(z)-.25f*(row%2)},
                        {.48f,.235f,.46f},b.random.next(-.04f,.04f));
        }
        for(int col=0;col<3;++col) for(int row=0;row<4-col;++row)
            b.stone({side*(2.7f+col*.85f),row*.55f+.24f,-2},{.405f,.26f,.54f});
    }
    for(int z=0;z<17;++z) for(int x=-1;x<=1;++x) {
        if(b.random.next(0,1)<.13f)continue;
        b.stone({x*.85f+b.random.next(-.08f,.08f),-.08f+b.random.next(-.025f,.025f),6-z*.84f},
                {.39f,.09f,.38f},b.random.next(-.08f,.08f));
    }
    for(int i=0;i<110;++i) {
        float x=b.random.next(-7,7),z=b.random.next(-12,2);
        if(std::abs(x)<1.4f)continue;
        float r=b.random.next(.12f,.44f);
        b.stone({x,elevation(x,z)+r*.5f,z},{r,r*.6f,r*.78f},b.random.next(0,pi));
    }
    for(int x=-32;x<=32;x+=5) for(int z=-32;z<=27;z+=5) {
        float px=x+b.random.next(-2.2f,2.2f),pz=z+b.random.next(-2.2f,2.2f);
        if(std::abs(px)<6.5f && pz>-14 && pz<14)continue;
        // Open the sunward edge of the clearing. Sun rays must reach the masonry,
        // not terminate in an uninterrupted roof of pine branches.
        if(px>2 && px<16 && pz>-1 && pz<18)continue;
        b.tree(px,pz,b.random.next(9,16));
    }
    for(int i=0;i<400;++i) {
        float spread=i<280?10.f:27.f;
        float x=b.random.next(-spread,spread),z=b.random.next(-spread-4,spread);
        if(std::abs(x)<1.5f && z>-13 && z<15)continue;
        if(std::abs(std::abs(x)-3.6f)<.65f && z>-11 && z<1)continue;
        if(x>3.2f && x<5.1f && z>3.5f && z<7.4f)continue;
        b.fern(x,z,b.random.next(.65f,1.6f));
    }
    for(int i=0;i<24;++i) {
        float x=(i%2?1.f:-1.f)*b.random.next(5,13),z=b.random.next(-14,9);
        float size=b.random.next(.5f,1.1f);
        b.rock({x,elevation(x,z),z},{size,size*.65f,size*.85f});
    }
    // Fallen wood and clusters of grass break up the clearing around the path.
    b.branch({-8,elevation(-8,3)+.3f,3},{-3.8f,elevation(-3.8f,5)+.24f,5},.32f,.24f,12);
    for(int i=0;i<650;++i) {
        float x=b.random.next(-11,11),z=b.random.next(-15,12);
        if(std::abs(x)<1.4f || (std::abs(std::abs(x)-3.6f)<.65f && z<1 && z>-11))continue;
        if(x>3.2f && x<5.1f && z>3.5f && z<7.4f)continue;
        float3 p{x,elevation(x,z),z};
        for(int j=0;j<7;++j) {
            float a=b.random.next(0,2*pi),h=b.random.next(.18f,.48f);
            b.leaf(p,{std::cos(a)*h*.65f,h,std::sin(a)*h*.65f},.016f,{.055f,.11f,.023f,.25f});
        }
    }
    // Ivy spills over the remaining masonry, using individual folded leaves.
    for(int i=0;i<36;++i) {
        float x=(i%2?1:-1)*b.random.next(2.8f,4.2f), z=b.random.next(-10,0);
        for(int j=0;j<16;++j) {
            float3 p{x+.12f*std::sin(j*.7f),2.0f-j*.12f,z};
            float a=b.random.next(0,2*pi);
            b.leaf(p,{.22f*std::cos(a),-.16f,.22f*std::sin(a)},.12f,{.025f,.085f,.018f,.3f});
        }
    }
    // Raised spring feeding the separate flowing-water display.
    b.rock({3.5f,elevation(3.5f,3.5f)+.45f,3.5f},{.55f,.8f,.55f});
    std::vector<Mesh> result;
    for(auto& [key,m]:b.batches) result.push_back(std::move(m));
    return result;
}
}
