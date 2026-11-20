// Dithered orange waves behind the top of the landing: Perlin noise (Stefan Gustavson) through an 8x8
// Bayer matrix, after the wii-tv launcher's backdrop (itself after React Bits' Dither).
(() => {
  "use strict";
  const canvas = document.querySelector(".dither-bg");
  if (!canvas) return;
  const gl = canvas.getContext("webgl2", { antialias: false, alpha: false });
  if (!gl) { canvas.remove(); return; }

  const WAVE_SPEED = 0.05, WAVE_FREQUENCY = 3, WAVE_AMPLITUDE = 0.3;
  const INTRO_SECONDS = 1.8, CELL_CSS = 2, REF_CSS = 1100; // REF_CSS: reference height for the wave size
  const PAPER = [0x12, 0x0f, 0x0b];   // --color-paper, oklch(17% 0.010 67)
  const ORANGE = [0xf2, 0x6a, 0x21];  // brand orange, drawn as is
  const DENSITY = 0.5;                // share of lit dots on a full crest (1 = solid)
  const CREST_LOW = 0.25, CREST_HIGH = 0.95; // only the wave crests (above LOW) light up

  const VERTEX = `#version 300 es
in vec2 aPosition;
void main() { gl_Position = vec4(aPosition, 0.0, 1.0); }`;

  const WAVE = `#version 300 es
precision highp float;
uniform vec2 uResolution; uniform float uCell; uniform float uTime; uniform float uIntro; uniform float uReach; uniform float uRef;
const float SPEED = ${WAVE_SPEED.toFixed(3)}, FREQ = ${WAVE_FREQUENCY.toFixed(3)}, AMP = ${WAVE_AMPLITUDE.toFixed(3)};
out vec4 outColor;
vec4 mod289(vec4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec4 permute(vec4 x) { return mod289(((x * 34.0) + 1.0) * x); }
vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }
vec2 fade(vec2 t) { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
float cnoise(vec2 P) {
  vec4 Pi = floor(P.xyxy) + vec4(0.0, 0.0, 1.0, 1.0);
  vec4 Pf = fract(P.xyxy) - vec4(0.0, 0.0, 1.0, 1.0);
  Pi = mod289(Pi);
  vec4 ix = Pi.xzxz; vec4 iy = Pi.yyww; vec4 fx = Pf.xzxz; vec4 fy = Pf.yyww;
  vec4 i = permute(permute(ix) + iy);
  vec4 gx = fract(i * (1.0 / 41.0)) * 2.0 - 1.0;
  vec4 gy = abs(gx) - 0.5;
  vec4 tx = floor(gx + 0.5);
  gx = gx - tx;
  vec2 g00 = vec2(gx.x, gy.x); vec2 g10 = vec2(gx.y, gy.y);
  vec2 g01 = vec2(gx.z, gy.z); vec2 g11 = vec2(gx.w, gy.w);
  vec4 norm = taylorInvSqrt(vec4(dot(g00, g00), dot(g01, g01), dot(g10, g10), dot(g11, g11)));
  g00 *= norm.x; g01 *= norm.y; g10 *= norm.z; g11 *= norm.w;
  float n00 = dot(g00, vec2(fx.x, fy.x)); float n10 = dot(g10, vec2(fx.y, fy.y));
  float n01 = dot(g01, vec2(fx.z, fy.z)); float n11 = dot(g11, vec2(fx.w, fy.w));
  vec2 fadeXY = fade(Pf.xy);
  vec2 nx = mix(vec2(n00, n01), vec2(n10, n11), fadeXY.x);
  return 2.3 * mix(nx.x, nx.y, fadeXY.y);
}
float fbm(vec2 p) {
  float value = 0.0; float amp = 1.0;
  for (int i = 0; i < 4; i++) { value += amp * abs(cnoise(p)); p *= FREQ; amp *= AMP; }
  return value;
}
float pattern(vec2 p) { vec2 p2 = p - uTime * SPEED; return fbm(p + fbm(p2)); }
void main() {
  vec2 corner = (gl_FragCoord.xy - 0.5) * uCell;
  // Wave size is set by the reference height (uRef) at the top: a taller canvas extends the waves, it does not enlarge them.
  vec2 uv = (corner - vec2(0.5 * uResolution.x, uResolution.y - 0.5 * uRef)) / uRef;
  float f = pattern(uv);
  float show = clamp((uIntro * uReach - length(uv)) / 0.45, 0.0, 1.0);
  float v = clamp(f, 0.0, 1.0) * show * show * (3.0 - 2.0 * show);
  outColor = vec4(floor(v * 255.0) / 255.0, fract(v * 255.0), 0.0, 1.0);
}`;

  const DITHER = `#version 300 es
precision highp float; precision highp int;
uniform sampler2D uField; uniform ivec2 uFieldSize; uniform float uCellDevice;
uniform vec3 uWave; uniform vec3 uBackground;
const float DENSITY = ${DENSITY.toFixed(3)};
const float CREST_LOW = ${CREST_LOW.toFixed(3)}, CREST_HIGH = ${CREST_HIGH.toFixed(3)};
out vec4 outColor;
const float bayer8[64] = float[64](
  0.0/64.0, 48.0/64.0, 12.0/64.0, 60.0/64.0, 3.0/64.0, 51.0/64.0, 15.0/64.0, 63.0/64.0,
  32.0/64.0, 16.0/64.0, 44.0/64.0, 28.0/64.0, 35.0/64.0, 19.0/64.0, 47.0/64.0, 31.0/64.0,
  8.0/64.0, 56.0/64.0, 4.0/64.0, 52.0/64.0, 11.0/64.0, 59.0/64.0, 7.0/64.0, 55.0/64.0,
  40.0/64.0, 24.0/64.0, 36.0/64.0, 20.0/64.0, 43.0/64.0, 27.0/64.0, 39.0/64.0, 23.0/64.0,
  2.0/64.0, 50.0/64.0, 14.0/64.0, 62.0/64.0, 1.0/64.0, 49.0/64.0, 13.0/64.0, 61.0/64.0,
  34.0/64.0, 18.0/64.0, 46.0/64.0, 30.0/64.0, 33.0/64.0, 17.0/64.0, 45.0/64.0, 29.0/64.0,
  10.0/64.0, 58.0/64.0, 6.0/64.0, 54.0/64.0, 9.0/64.0, 57.0/64.0, 5.0/64.0, 53.0/64.0,
  42.0/64.0, 26.0/64.0, 38.0/64.0, 22.0/64.0, 41.0/64.0, 25.0/64.0, 37.0/64.0, 21.0/64.0);
void main() {
  vec2 cell = floor(gl_FragCoord.xy / uCellDevice);
  vec4 s = texelFetch(uField, clamp(ivec2(cell), ivec2(0), uFieldSize - 1), 0);
  float crest = smoothstep(CREST_LOW, CREST_HIGH, s.r + s.g / 255.0);
  float f = crest * DENSITY;
  ivec2 c = ivec2(mod(cell, 8.0));
  float threshold = bayer8[c.y * 8 + c.x] + 0.5 / 64.0;
  // A dot is lit (exact orange) or not (background): no in-between colour that would change the hue.
  outColor = vec4(f > threshold ? uWave : uBackground, 1.0);
}`;

  const srgb = rgb => rgb.map(v => v / 255);
  const background = srgb(PAPER);
  const waveRgb = srgb(ORANGE);

  function program(fs) {
    const p = gl.createProgram();
    for (const [type, src] of [[gl.VERTEX_SHADER, VERTEX], [gl.FRAGMENT_SHADER, fs]]) {
      const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
      gl.attachShader(p, s);
    }
    gl.bindAttribLocation(p, 0, "aPosition"); gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
    return p;
  }
  let wave, dither;
  try { wave = program(WAVE); dither = program(DITHER); } catch (e) { canvas.remove(); return; }
  const U = (p, n) => gl.getUniformLocation(p, n);

  const vao = gl.createVertexArray(); gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);

  const tex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  const fbo = gl.createFramebuffer();

  let width = 1, height = 1, cell = 1, cols = 1, rows = 1, ref = 1;
  function resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(canvas.clientWidth * dpr)), h = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (w === width && h === height) return;
    width = canvas.width = w; height = canvas.height = h;
    cell = Math.max(1, Math.round(CELL_CSS * dpr));
    ref = Math.min(height, Math.round(REF_CSS * dpr));
    cols = Math.ceil(width / cell); rows = Math.ceil(height / cell);
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, cols, rows, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  function render(time, intro) {
    resize();
    gl.bindVertexArray(vao);
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo); gl.viewport(0, 0, cols, rows);
    gl.useProgram(wave);
    gl.uniform2f(U(wave, "uResolution"), width, height);
    gl.uniform1f(U(wave, "uCell"), cell);
    gl.uniform1f(U(wave, "uTime"), time);
    gl.uniform1f(U(wave, "uIntro"), intro);
    gl.uniform1f(U(wave, "uRef"), ref);
    gl.uniform1f(U(wave, "uReach"), Math.hypot(0.5 * width / ref, Math.max(0.5, (height - 0.5 * ref) / ref)) + 0.45);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null); gl.viewport(0, 0, width, height);
    gl.useProgram(dither);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.uniform1i(U(dither, "uField"), 0);
    gl.uniform2i(U(dither, "uFieldSize"), cols, rows);
    gl.uniform1f(U(dither, "uCellDevice"), cell);
    gl.uniform3fv(U(dither, "uWave"), waveRgb);
    gl.uniform3fv(U(dither, "uBackground"), background);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  // Reduced motion: one still frame, redrawn on resize.
  const still = window.matchMedia("(prefers-reduced-motion: reduce)");
  let time = 8, reveal = 0, last = 0, frame = 0, raf = 0, visible = true, paused = false;
  const tick = now => {
    raf = requestAnimationFrame(tick);
    if ((frame++ & 1) !== 0) return; // half the display rate is enough for slow waves
    const dt = last ? Math.max(0.0005, Math.min(0.05, (now - last) / 1000)) : 1 / 30;
    last = now; time += dt;
    if (reveal < 1) reveal = Math.min(1, reveal + dt / INTRO_SECONDS);
    render(time, reveal * reveal * (3 - 2 * reveal));
  };
  const start = () => { if (!raf && visible && !paused && !document.hidden && !still.matches) { last = 0; raf = requestAnimationFrame(tick); } };
  const stop = () => { cancelAnimationFrame(raf); raf = 0; };
  const sync = () => { if (still.matches) { stop(); render(time, 1); } else if (visible && !paused && !document.hidden) start(); else stop(); };

  new IntersectionObserver(([e]) => { visible = e.isIntersecting; sync(); }).observe(canvas);
  document.addEventListener("visibilitychange", sync);
  document.addEventListener("weft:pause", e => { paused = e.detail; sync(); }); // the scene's Pause button
  still.addEventListener("change", sync);
  window.addEventListener("resize", () => { if (!raf) render(time, 1); });
  sync();
})();
