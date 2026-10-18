// A blend code holds the LayerBlend discriminant in bits 0-7, the Perceptual
// blend space in bit 8 and float documents in bit 9.
const BLEND_PERCEPTUAL: u32 = 256u;
const BLEND_FLOAT: u32 = 512u;
const BLEND_DIVISOR: f32 = .00006103515625;
fn blend_mode(code: u32) -> u32 { return code & 255u; }
