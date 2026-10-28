import bevy_pbr::render::{forward_io::{Vertex, decompress_vertex}, mesh_functions,
    mesh_view_bindings::{globals, view}, view_transformations::position_world_to_clip};
struct Settings {kind: u32, order: array<vec4<u32>,27>}
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings: Settings;
const kind: u32 = constants::EFFECT_KIND;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
fn random(seed: u32) -> f32 {
    var x = seed;
    x = ((x ^ (x >> 16u)) * 0x7feb352du);
    x = ((x ^ (x >> 15u)) * 0x846ca68bu);
    x = x ^ (x >> 16u);
    return f32(x >> 8u)/16777216.0;
}
fn pulse(t: f32) -> f32 {
    let p = t % 4.8;
    let a=(p-0.12)/0.055; let b=(p-0.3)/0.08;
    return exp(-a*a)+0.75*exp(-b*b);
}
fn bolt(i: u32, strike: u32) -> vec3<f32> {
    let end = i == 0u || i == 18u;
    return vec3(select((random(i+strike*73u+891u)-0.5)*0.8,0.0,end),
        5.2*(1.0-f32(i)/18.0), select((random(i+strike*91u+477u)-0.5)*0.5,0.0,end));
}
@vertex
fn vertex(input: Vertex) -> Output {
    let v = decompress_vertex(input,input.instance_index);
    let slot = u32(v.position.x);
    let i = settings.order[slot/4u][slot%4u];
    let t = globals.time;
    let tau = 6.28318530718;
    var p = vec3(0.0);
    var size = vec2(1.0);
    var color = vec4(1.0);
    var ribbon = false;
    var a = vec3(0.0);
    var b = vec3(0.0);
    if kind == 0u {
        if i < 54u {
            let seed = random(i+1u);
            let age = fract(t*0.75+seed);
            let angle = random(i+91u)*tau;
            let radius = (1.0-age)*0.35;
            p = vec3(cos(angle)*radius+0.15*sin(t*2.0+seed*12.0),0.12+age*1.9,sin(angle)*radius);
            size = vec2(0.48*(1.0-age)+0.16,0.7*(1.0-age)+0.35);
            color = vec4(1.0,0.12+0.32*(1.0-age),0.015,sin(3.14159265*age)*0.62);
        } else {
            let seed = random(i-54u+901u);
            let age = fract(t*0.3+seed);
            let angle = seed*tau;
            p = vec3(0.45*sin(angle+age*3.0),age*3.0,0.4*cos(angle));
            size = vec2(0.025,0.09);
            color = vec4(1.0,0.4,0.06,(1.0-age)*0.85);
        }
    } else if kind == 1u {
        let age = t % 5.4;
        let fade = clamp(1.0-age/2.5,0.0,1.0);
        if i < 72u {
            let angle = random(i+1301u)*tau;
            let lift = random(i+701u);
            let radius = sqrt(min(age,2.5))*1.25;
            p = vec3(cos(angle)*radius*(0.35+lift*0.65),0.6+age*(1.4+lift*2.6),sin(angle)*radius);
            size = vec2(0.2+min(age,2.5)*(0.5+lift*0.4));
            color = vec4(1.0,0.1+0.3*fade,0.012,fade*0.65);
        } else {
            let n = i-72u;
            let angle = random(n+4101u)*tau;
            let speed = 1.6+random(n+151u)*2.0;
            let age2 = min(age,2.0);
            p = vec3(cos(angle)*age2*speed,max(0.04,age2*(2.5+random(n+9u)*2.0)-2.6*age2*age2),sin(angle)*age2*speed);
            size = vec2(0.035,0.15);
            color = vec4(1.0,0.6,0.1,clamp(1.0-age/1.5,0.0,1.0));
        }
    } else if kind == 2u {
        let seed = random(i+2001u);
        let age = fract(t*1.5+seed);
        let angle = seed*tau;
        p = vec3(0.75+cos(angle)*age*0.7,-0.07+max(0.0,age*1.3-1.25*age*age),1.45+sin(angle)*age*0.7);
        size = vec2(0.04+0.08*age,0.08+0.09*age);
        color = vec4(0.6,0.8,0.86,(1.0-age)*0.75);
    } else if kind == 3u {
        ribbon = true;
        let strike = u32(floor(t/4.8));
        size.x = 0.1;
        color = vec4(0.5,0.7,1.0,pulse(t));
        if i < 18u { a = bolt(i,strike); b = bolt(i+1u,strike); }
        else {
            let branch = (i-18u)/6u;
            let step = (i-18u)%6u;
            a = bolt(4u+branch*4u,strike);
            for(var j=0u;j<=step;j++) {
                b = a+vec3(select(-1.0,1.0,branch%2u==1u)*(0.13+random(j+branch*11u)*0.22),
                    -0.28,(random(j+2u)*2.0-1.0)*0.25);
                if j < step {a=b;}
            }
            size.x = 0.045;
            color = vec4(0.35,0.55,1.0,pulse(t)*(1.0-f32(step)/7.0));
        }
    } else {
        let seed = random(i+3001u);
        let age = fract(t*0.12+seed);
        let angle = random(i+1801u)*tau;
        p = vec3(age*1.1+sin(angle+t*0.3)*(0.15+age*0.55),0.35+age*5.5,cos(angle+age*2.0)*(0.2+age*0.55));
        size = vec2(0.75+age*1.8,(0.75+age*1.8)*1.15);
        let shade = 0.055+age*0.08;
        color = vec4(shade,shade*1.01,shade*1.04,sin(3.14159265*age)*0.7);
    }
    let model = mesh_functions::get_world_from_local(input.instance_index);
    let center = model[3].xyz;
    var world: vec3<f32>;
    if ribbon {
        a += center; b += center;
        let side = normalize(cross(b-a,view.world_position-(a+b)*0.5))*size.x*0.5;
        world = mix(a,b,v.uv.y)+side*(v.uv.x*2.0-1.0);
    } else {
        p += center;
        let direction = normalize(view.world_position-p);
        let right = normalize(cross(vec3(0.0,1.0,0.0),direction));
        let up = normalize(cross(direction,right));
        world = p + right*(v.uv.x-0.5)*size.x + up*(v.uv.y-0.5)*size.y;
    }
    var out: Output;
    out.position = position_world_to_clip(world);
    out.uv = v.uv;
    out.color = color;
    return out;
}
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);
}
fn noise(p: vec2<f32>) -> f32 {
    let i=floor(p); var f=fract(p); f=f*f*(3.0-2.0*f);
    return mix(mix(hash(i),hash(i+vec2(1.0,0.0)),f.x),mix(hash(i+vec2(0.0,1.0)),hash(i+vec2(1.0)),f.x),f.y);
}
fn turbulence(p: vec2<f32>) -> f32 {return noise(p)*0.57+noise(p*2.03+7.0)*0.28+noise(p*4.11)*0.15;}
@fragment
fn fragment(in: Output) -> @location(0) vec4<f32> {
    if in.color.a<0.003 {discard;}
    let p=in.uv*2.0-1.0;
    var color=in.color.rgb;
    var mask=0.0; var energy=1.0;
    if kind==0u {
        let taper=1.0-in.uv.y*0.65;
        let radial=1.0-smoothstep(0.15,1.0,length(vec2(p.x/taper,p.y)));
        if radial*in.color.a<0.003 {discard;}
        let n=turbulence(in.uv*vec2(4.0,5.0)-vec2(0.0,globals.time*2.8));
        mask=radial*smoothstep(0.15,0.65,n);
        energy=1.25;
    } else if kind==1u {
        let radial=1.0-smoothstep(0.35,1.0,length(p));
        if radial*in.color.a<0.003 {discard;}
        let n=turbulence(in.uv*5.0-vec2(globals.time*0.6));
        mask=radial*smoothstep(0.1,0.7,n);
        color*=mix(0.35,1.0,n); energy=1.56;
    } else if kind==2u {mask=1.0-smoothstep(0.15,1.0,length(p));}
    else if kind==3u {
        let core=1.0-smoothstep(0.03,0.18,abs(p.x)); mask=exp(-abs(p.x)*4.0);
        color=mix(color,vec3(0.92,0.96,1.0),core); energy=7.3*(0.3+core*0.7);
    } else {
        let radial=1.0-smoothstep(0.25,1.0,length(p));
        if radial*in.color.a<0.003 {discard;}
        let n=turbulence(in.uv*4.0+vec2(globals.time*0.12,-globals.time*0.2));
        mask=radial*smoothstep(0.12,0.7,n);
        color*=0.5+0.9*n+0.25*in.uv.y;
    }
    let alpha=clamp(mask*in.color.a,0.0,1.0); if alpha<0.003 {discard;}
    // HDR camera exposure is fixed at 0.65, matching the reference's multiplier.
    return vec4(color*energy*0.65,alpha);
}
