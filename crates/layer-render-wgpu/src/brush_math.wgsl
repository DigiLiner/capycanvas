fn rotate(point: vec2<f32>, cosine: f32, sine: f32) -> vec2<f32> {
    return vec2<f32>(
        point.x * cosine - point.y * sine,
        point.x * sine + point.y * cosine,
    );
}
