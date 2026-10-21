struct Params { progress: f32, softness: f32 }

// A title carries a per-word order across its canvas - reading order, 0 for
// the first word and 1 for the last - and `reveal_order(uv)` reads it. Each
// word appears when Progress passes its own order, over `softness` of travel,
// so the words rise through the sentence rather than all at once.
//
// The orders are scaled into [0, 1 - softness] first: the last word's order is
// exactly 1, and a ramp that ends at 1 would need a progress past its own end
// to show it - at Progress 1 the final word stayed invisible.
//
// On a clip with no map the order is 0 everywhere, so every pixel is revealed
// at any progress above 0: the effect is a no-op on anything but a title.
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let edge = max(params.softness, 0.001);
    let order = reveal_order(uv) * (1.0 - edge);
    let shown = smoothstep(order, order + edge, params.progress);
    let c = sample(uv);
    return vec4<f32>(c.rgb, c.a * shown);
}
