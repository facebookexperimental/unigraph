// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useEffect, useRef } from "react";

// ─── WGSL Shader ────────────────────────────────────────────────────────────

const SHADER = /* wgsl */ `
// Adapted from one of the @XorDev shaders
//
// SOURCE: https://github.com/XorDev/Singularity/blob/main/Shaders/shadertoy-version.glsl
// LICENSE: https://github.com/XorDev/Singularity/blob/main/LICENSE

struct Uniforms {
  time: f32,         // elapsed seconds
  _pad: f32,         // alignment padding
  resolution: vec2f, // canvas pixel dimensions
}
@group(0) @binding(0) var<uniform> u: Uniforms;

// Fullscreen triangle — 3 vertices covering all of clip space. No vertex
// buffer needed; positions come from the vertex index.
@vertex
fn vs(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4f {
  var pos = array<vec2f, 3>(
    vec2f(-1.0, -1.0),
    vec2f( 3.0, -1.0),
    vec2f(-1.0,  3.0),
  );
  return vec4f(pos[idx], 0.0, 1.0);
}

// Near-black maroon -> deep crimson -> scarlet -> hot scarlet, cross-faded
// with smoothstep so the ramp has no banding. Every colour in the image
// comes from here or from the rim tint below.
fn palette(x: f32) -> vec3f {
  let s: f32 = clamp(x, 0.0, 1.0);
  var c: vec3f = mix(vec3f(0.020, 0.002, 0.006), vec3f(0.180, 0.010, 0.022),
                     smoothstep(0.00, 0.34, s));
  c = mix(c, vec3f(0.480, 0.035, 0.055), smoothstep(0.32, 0.62, s));
  c = mix(c, vec3f(0.880, 0.110, 0.080), smoothstep(0.60, 0.84, s));
  c = mix(c, vec3f(1.000, 0.280, 0.160), smoothstep(0.86, 1.00, s));
  return c;
}

@fragment
fn fs(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let r: vec2f = u.resolution;
  // WebGPU's fragment position is y-down; flip to match the GLSL original.
  let FC: vec2f = vec2f(pos.x, r.y - pos.y);
  let t: f32 = u.time * 0.35;

  // Camera at the origin looking down -z; the volume is centred at z = -8.
  let rd: vec3f = normalize(vec3f((FC * 2.0 - r) / r.y, -1.027));

  // The domain warp is axis-aligned, so the field is spun in its own frame.
  // Rotation preserves length, leaving the envelope round as it tumbles.
  let th: f32 = u.time * 0.22;
  let cth: f32 = cos(th);
  let sth: f32 = sin(th);
  let lightDir: vec3f = normalize(vec3f(-0.4, 0.7, 0.55));

  var col = vec3f(0.0, 0.0, 0.0);
  var trans: f32 = 1.0; // transmittance, accumulated front-to-back
  var z: f32 = 1.0;

  for (var i: f32 = 0.0; i < 50.0; i += 1.0) {
    var p: vec3f = z * rd;
    p.z += 8.0;
    let rr: f32 = length(p);

    // Density envelope: a compact core plus a wide thin atmosphere that
    // reaches past the rim.
    let env: f32 = exp(-pow(rr / 2.3, 2.2)) + 0.8 * exp(-pow(rr / 3.35, 3.0));
    if (env > 0.001) {
      var a: vec3f = vec3f(p.x * cth + p.z * sth, p.y, -p.x * sth + p.z * cth);

      // Domain warp — 11 octaves of decreasing amplitude. The phase does not
      // vary with i; a per-step offset would decorrelate the samples along
      // each ray and average the field into flat haze.
      var d: f32 = 2.0;
      for (var k: i32 = 0; k < 11; k = k + 1) {
        a += sin(a * d + vec3f(t)).yzx / d;
        d += 1.0;
      }

      // Filament noise. length(sin(..)+1) spans [0, 2*sqrt(3)]; normalise,
      // gamma up for contrast, then floor so the silhouette follows the
      // round envelope rather than the filaments.
      var n: f32 = length(sin(a / 0.26 + vec3f(z)) + vec3f(1.0)) / 3.4641;
      n = 0.08 + 0.92 * pow(n, 2.9);

      let dens: f32 = env * n * 2.0;
      // Hot scarlet through the middle, falling to near-black crimson at
      // the edge, with the filaments shifting it either way.
      let hue: f32 = 1.02 - 0.5 * pow(rr / 3.35, 2.6) + (n - 0.4) * 0.35;
      // Extra radiance in the core, plus a directional light so the volume
      // reads as a sphere rather than a flat disc.
      let core: f32 = 1.0 + 0.75 * exp(-pow(rr / 2.0, 2.0));
      let ndl: f32 = dot(p / max(rr, 1e-4), lightDir);
      let shade: f32 = 0.72 + 0.48 * (0.5 + 0.5 * ndl);

      col += palette(hue) * (dens * trans * 0.28 * 0.9 * core * shade);
      trans *= exp(-dens * 0.28 * 0.6);
    }
    z += 0.28;
  }

  let rad: f32 = length((FC - 0.5 * r) / r.y);
  let R: f32 = 0.22;

  // Contain the volume within the disc, plus a soft exponential term that
  // lets some plasma through past the rim.
  let hard: f32 = 1.0 - smoothstep(R, R * 1.10, rad);
  let spill: f32 = 0.85 * exp(-max(rad - R, 0.0) * 8.0);
  col *= min(hard + spill, 1.0);

  // Rim highlight: sharply peaked at R but continuous, so the circle reads
  // crisp without a hard edge. Tinted hotter and lighter than the plasma so
  // it separates by luminance, everything being one hue. The exponential
  // carries it outward into a bloom.
  let limb: f32 = 0.011 / (0.010 + abs(rad - R));
  let bloom: f32 = exp(-max(rad - R, 0.0) * 17.0);
  col += vec3f(1.00, 0.44, 0.30) * limb * 1.85
    + vec3f(0.50, 0.07, 0.05) * bloom * 0.18;

  // The rim's 1/|r-R| tail decays too slowly to reach black on its own,
  // which would show the canvas as a grey square. Force it to exactly zero
  // inside the canvas bounds.
  col *= 1.0 - smoothstep(0.34, 0.47, rad);

  // Hue-preserving tone map: compress the magnitude and keep the channel
  // ratio, so the hot core stays scarlet instead of clipping to white.
  let m: f32 = max(col.r, max(col.g, col.b));
  let compressed: f32 = tanh(m * 0.6);

  // Alpha tracks content, so empty pixels are transparent and the canvas
  // never reads as a box. Premultiplied — the context is configured to match.
  return vec4f(col * (compressed / max(m, 1e-5)), compressed);
}
`;

// ─── Canvas dimensions (CSS pixels) ─────────────────────────────────────────

const W = 700;
const H = 700;

// ─── WebGPU init ────────────────────────────────────────────────────────────

async function initWebGPU(
  canvas: HTMLCanvasElement,
): Promise<(() => void) | null> {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const nav = navigator as any;
  if (!nav.gpu) return null;

  const adapter = await nav.gpu.requestAdapter();
  if (!adapter) return null;

  const device = await adapter.requestDevice();
  const ctx = canvas.getContext("webgpu") as any;
  if (!ctx) return null;

  // The volume march is the expensive part; cap the ratio so a retina
  // display doesn't more than double the cost for a loading screen.
  const dpr = Math.min(window.devicePixelRatio ?? 1, 1.5);
  canvas.width = Math.round(W * dpr);
  canvas.height = Math.round(H * dpr);

  const format = nav.gpu.getPreferredCanvasFormat();
  // Premultiplied, to match the shader's content-driven alpha.
  ctx.configure({ device, format, alphaMode: "premultiplied" });

  const module = device.createShaderModule({ code: SHADER });

  const pipeline = device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vs" },
    fragment: { module, entryPoint: "fs", targets: [{ format }] },
    primitive: { topology: "triangle-list" },
  });

  // Uniform buffer: [time: f32, _pad: f32, resolution: vec2f] = 16 bytes
  const uniformBuffer = device.createBuffer({
    size: 16,
    usage: 0x0040 /* UNIFORM */ | 0x0008 /* COPY_DST */,
  });

  const bindGroup = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: { buffer: uniformBuffer } }],
  });

  let raf: number;
  const t0 = performance.now();
  const uniformData = new Float32Array(4);

  const frame = () => {
    uniformData[0] = (performance.now() - t0) / 1000;
    uniformData[1] = 0;
    uniformData[2] = canvas.width;
    uniformData[3] = canvas.height;
    device.queue.writeBuffer(uniformBuffer, 0, uniformData);

    const encoder = device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: ctx.getCurrentTexture().createView(),
          loadOp: "clear",
          storeOp: "store",
          clearValue: { r: 0, g: 0, b: 0, a: 0 },
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(3);
    pass.end();
    device.queue.submit([encoder.finish()]);

    raf = requestAnimationFrame(frame);
  };
  raf = requestAnimationFrame(frame);

  return () => {
    cancelAnimationFrame(raf);
    uniformBuffer.destroy();
    device.destroy();
  };
}

// ─── Component ──────────────────────────────────────────────────────────────

export function GraphLoadingAnimation() {
  const ref = useRef<HTMLCanvasElement>(null);
  const fallbackRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;

    let cleanup: (() => void) | null = null;
    let cancelled = false;

    initWebGPU(canvas).then((dispose) => {
      if (cancelled) {
        dispose?.();
        return;
      }
      if (dispose) {
        cleanup = dispose;
      } else if (fallbackRef.current) {
        // WebGPU unavailable — show CSS fallback
        canvas.style.display = "none";
        fallbackRef.current.style.display = "flex";
      }
    });

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, []);

  return (
    <div className="h-screen flex flex-col items-center justify-center gap-6 bg-black">
      <canvas ref={ref} aria-hidden="true" style={{ width: W, height: H }} />
      <div
        ref={fallbackRef}
        style={{ display: "none" }}
        className="flex-col items-center justify-center gap-6"
      >
        <div
          aria-hidden="true"
          className="rounded-full border-4 border-primary/30 border-t-primary animate-spin"
          style={{ width: 48, height: 48 }}
        />
      </div>
      <p role="status" className="text-sm text-neutral-400 animate-pulse">
        Loading graph…
      </p>
    </div>
  );
}
