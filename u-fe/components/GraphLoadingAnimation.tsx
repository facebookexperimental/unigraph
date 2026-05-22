// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useEffect, useRef } from "react";

// ─── WGSL Shader ────────────────────────────────────────────────────────────

const SHADER = /* wgsl */ `
// Adapted from one of the @XorDev shaders
//
// SOURCE: https://github.com/XorDev/Singularity/blob/main/Shaders/shadertoy-version.glsl
// LICENSE: https://github.com/XorDev/Singularity/blob/main/LICENSE

// Uniforms passed from JS each frame
struct Uniforms {
  time: f32,        // elapsed seconds
  _pad: f32,        // alignment padding
  resolution: vec2f, // canvas pixel dimensions
}
@group(0) @binding(0) var<uniform> u: Uniforms;

// Fullscreen triangle — 3 vertices that cover the entire clip space.
// No vertex buffer needed; positions are computed from vertex index.
@vertex
fn vs(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4f {
  var pos = array<vec2f, 3>(
    vec2f(-1.0, -1.0),
    vec2f( 3.0, -1.0),
    vec2f(-1.0,  3.0),
  );
  return vec4f(pos[idx], 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4f) -> @location(0) vec4f {

  // ── Coordinate setup ──────────────────────────────────────────────
  // Center pixel coords at origin, normalize by height for aspect ratio,
  // then divide by scale factor (smaller = smaller render, more whitespace).
  let p = (pos.xy * 2.0 - u.resolution) / u.resolution.y / 0.5;

  // Diagonal vector — used later for asymmetric disk scaling.
  let d = vec2f(-1.0, 1.0);

  // Working position — no perspective skew, just centered coords.
  let c = p;

  // ── Spiral coordinate transform ───────────────────────────────────
  // Squared distance from center — controls radial brightness falloff.
  let a = dot(c, c);

  // Rotation angle: log(a) maps distance into log-polar space (creates spiral),
  // time term adds continuous rotation.
  let angle = 0.5 * log(a) + u.time * 0.1;

  // Rotation matrix via cosine phase trick (avoids calling sin()):
  //   cos(angle + 0)  ≈  cos(angle)
  //   cos(angle + 33) ≈ -sin(angle)   [33 rad ≈ 10.5π, phase-shifted]
  //   cos(angle + 30) ≈  sin(angle)   [30 rad ≈  9.5π, phase-shifted]
  // Result: approximate 2D rotation matrix.
  let cv = cos(vec4f(angle, angle + 33.0, angle + 30.0, angle));
  let spiral_rot = mat2x2f(cv.x, cv.y, cv.z, cv.w);

  // Apply spiral rotation, scale up for detail.
  var v = (c * spiral_rot) / 0.18;

  // Wave accumulator — each iteration adds a sine wave layer.
  var w = vec2f(0.0);

  // ── Wave generation loop ──────────────────────────────────────────
  // 8 iterations of domain warping. Each pass:
  //   1. Distorts v with frequency-scaled sine (v.yx swaps axes → rotation)
  //   2. Divides by i for 1/f noise (lower frequencies dominate)
  //   3. Accumulates wave heights into w
  // The result: complex organic flow patterns in v, brightness map in w.
  var i: f32 = 0.2;
  loop {
    let prev = i;
    i += 1.0;
    if (prev >= 8.0) { break; }
    v += 0.7 * sin(v.yx * i + u.time) / i + 0.45;
    w += 1.0 + sin(v);
  }

  // ── Accretion disk radius ─────────────────────────────────────────
  // High-frequency oscillation from the flow field (sin(v/0.3)),
  // combined with asymmetrically scaled position (c * (3+d) = c * vec2(2,4)).
  // The length gives distance to the warped disk shape.
  i = length(sin(v / 0.3) * 0.4 + c * (3.0 + d));

  // ── Final color assembly ──────────────────────────────────────────
  // Each divisor in this chain controls a different visual aspect:

  // Horizontal color gradient: red grows rightward (+0.6),
  // green/blue decrease (−0.4, −0.7). Creates warm↔cool split.
  let color_gradient = exp(c.x * vec4f(0.6, -0.4, -0.7, 0.0));

  // Wave texture: bright in wave gaps (small w), dark in wave peaks.
  // .xyyx swizzle maps 2D waves → 4 color channels with crossover.
  let wave_texture = w.xyyx;

  // Disk brightness: quadratic in disk distance.
  // Bright ring where i is small, dark further away.
  let disk_brightness = vec4f(8.0 + i * i / 5.0 - i);

  // Radial falloff — creates the donut-on-white shape:
  //   0.5/a → huge near center → divides energy to zero → white center
  //   a*1.0 → grows outward → divides energy to zero → white edges
  //   5.0   → base offset controlling overall brightness
  let radial_falloff = vec4f(5.0 + 0.5 / a + a * 1.0);

  // Rim highlight at radius 0.9 — the accretion disk itself.
  // Denominator is tiny right at length(p)=0.9, creating a bright ring.
  let rim = vec4f(0.018 + abs(length(p) - 0.9));

  // Multiply all energy contributions together via chained division.
  let energy = color_gradient / wave_texture / disk_brightness / radial_falloff / rim;

  // Soft-clamp via 1−exp(−x): maps [0,∞) → [0,1). More energy → brighter.
  let col = vec4f(1.0) - exp(-energy);

  // ── Brand color blend ─────────────────────────────────────────────
  // Compute luminance to identify dark vs bright areas.
  let lum = dot(col.rgb, vec3f(1.0, 0.1, 0.1));

  let brand = vec3f(0.98, 0.17, 0.21);
  let colored = mix(brand, vec3f(1.0), smoothstep(0.0, 0.6, lum));

  // Where luminance is near zero (outside the effect), output pure white.
  let mask = smoothstep(0.01, 0.08, lum);
  let final_color = mix(vec3f(1.0), colored, mask);

  return vec4f(final_color, 1.0);
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

  const dpr = Math.min(window.devicePixelRatio ?? 1, 2);
  canvas.width = W * dpr;
  canvas.height = H * dpr;

  const format = nav.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format, alphaMode: "opaque" });

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
          clearValue: { r: 1, g: 1, b: 1, a: 1 },
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
    <div className="h-screen flex flex-col items-center justify-center gap-6 bg-white">
      <canvas ref={ref} style={{ width: W, height: H }} />
      <div
        ref={fallbackRef}
        style={{ display: "none" }}
        className="flex-col items-center justify-center gap-6"
      >
        <div
          className="rounded-full border-4 border-primary/30 border-t-primary animate-spin"
          style={{ width: 48, height: 48 }}
        />
      </div>
      <p className="text-sm text-neutral-400 animate-pulse">Loading graph…</p>
    </div>
  );
}
