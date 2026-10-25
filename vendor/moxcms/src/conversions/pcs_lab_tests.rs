use super::*;
use crate::{
    LutDataType, LutMultidimensionalType, LutStore, LutType, Matrix3d, ProfileVersion,
    ToneReprCurve, Vector3d,
};

fn table(kind: LutType) -> LutWarehouse {
    let store = |values: &[u16]| match kind {
        LutType::Lut8 => LutStore::Store8(values.iter().map(|v| (v / 257) as u8).collect()),
        LutType::Lut16 => LutStore::Store16(values.to_vec()),
        _ => unreachable!(),
    };
    let entries = if kind == LutType::Lut8 { 256 } else { 2 };
    let identity: Vec<_> = (0..entries)
        .map(|i| (u32::from(i) * 65535 / u32::from(entries - 1)) as u16)
        .collect();
    LutWarehouse::Lut(LutDataType {
        num_input_channels: 3,
        num_output_channels: 3,
        num_clut_grid_points: 2,
        matrix: Matrix3d::IDENTITY,
        num_input_table_entries: entries,
        num_output_table_entries: entries,
        input_table: store(&identity.repeat(3)),
        clut_table: store(
            &(0..8)
                .flat_map(|i| [i / 4, i / 2 % 2, i % 2].map(|v| v * 65535))
                .collect::<Vec<_>>(),
        ),
        output_table: store(&identity.repeat(3)),
        lut_type: kind,
    })
}

fn profile(version: ProfileVersion, lut: LutWarehouse) -> ColorProfile {
    let mut profile = ColorProfile::new_srgb();
    profile.pcs = DataColorSpace::Lab;
    profile.version_internal = version;
    profile.lut_a_to_b_perceptual = Some(lut.clone());
    profile.lut_b_to_a_perceptual = Some(lut);
    profile
}

fn check(
    profile: &ColorProfile,
    intent: RenderingIntent,
    direction: TransformDirection,
    expected: [f32; 3],
) {
    let input = [0.25, 0.5, 0.75];
    let mut sampled = input;
    match direction {
        TransformDirection::DeviceToPcs => pcs_lab_v2_to_v4(profile, &mut sampled, intent),
        TransformDirection::PcsToDevice => pcs_lab_v4_to_v2(profile, &mut sampled, intent),
    }
    for (actual, expected) in sampled.into_iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-7,
            "{intent:?} {direction:?}: {actual} vs {expected}"
        );
    }
    #[cfg(feature = "any_to_any")]
    {
        let stage = match direction {
            TransformDirection::DeviceToPcs => katana_pcs_lab_v2_to_v4(profile, intent),
            TransformDirection::PcsToDevice => katana_pcs_lab_v4_to_v2(profile, intent),
        };
        let mut input = input.to_vec();
        let output = stage.stage(&mut input).unwrap();
        for (actual, expected) in output.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-7);
        }
    }
}

fn expected(legacy: bool, direction: TransformDirection) -> [f32; 3] {
    let scale = if !legacy {
        1.
    } else {
        match direction {
            TransformDirection::DeviceToPcs => 65535. / 65280.,
            TransformDirection::PcsToDevice => 65280. / 65535.,
        }
    };
    [0.25 * scale, 0.5 * scale, 0.75 * scale]
}

#[test]
fn lab_pcs_samples_follow_table_encoding_in_v2_and_v4() {
    for version in [
        ProfileVersion::V2_1,
        ProfileVersion::V2_4,
        ProfileVersion::V4_0,
        ProfileVersion::V4_4,
    ] {
        for kind in [LutType::Lut8, LutType::Lut16] {
            let profile = profile(version, table(kind));
            for intent in [
                RenderingIntent::Perceptual,
                RenderingIntent::RelativeColorimetric,
                RenderingIntent::AbsoluteColorimetric,
                RenderingIntent::Saturation,
            ] {
                for direction in [
                    TransformDirection::DeviceToPcs,
                    TransformDirection::PcsToDevice,
                ] {
                    check(
                        &profile,
                        intent,
                        direction,
                        expected(kind == LutType::Lut16, direction),
                    );
                }
            }
        }
    }
}

#[test]
fn lab_pcs_uses_selected_direction_intent_and_perceptual_fallback() {
    let mut profile = profile(ProfileVersion::V2_1, table(LutType::Lut8));
    profile.lut_b_to_a_perceptual = Some(table(LutType::Lut16));
    profile.lut_a_to_b_colorimetric = Some(table(LutType::Lut16));
    profile.lut_b_to_a_colorimetric = Some(table(LutType::Lut8));
    for direction in [
        TransformDirection::DeviceToPcs,
        TransformDirection::PcsToDevice,
    ] {
        for intent in [
            RenderingIntent::RelativeColorimetric,
            RenderingIntent::AbsoluteColorimetric,
        ] {
            check(
                &profile,
                intent,
                direction,
                expected(direction == TransformDirection::DeviceToPcs, direction),
            );
        }
        check(
            &profile,
            RenderingIntent::Saturation,
            direction,
            expected(direction == TransformDirection::PcsToDevice, direction),
        );
    }
}

#[test]
fn modern_luts_and_xyz_pcs_do_not_receive_legacy_lab_scaling() {
    let curves = vec![ToneReprCurve::Parametric(vec![1.]); 3];
    let lut = LutWarehouse::Multidimensional(LutMultidimensionalType {
        num_input_channels: 3,
        num_output_channels: 3,
        grid_points: [0; 16],
        clut: None,
        a_curves: vec![],
        b_curves: curves,
        m_curves: vec![],
        matrix: Matrix3d::IDENTITY,
        bias: Vector3d { v: [0.; 3] },
    });
    let modern = profile(ProfileVersion::V2_1, lut);
    let mut xyz = profile(ProfileVersion::V2_1, table(LutType::Lut16));
    xyz.pcs = DataColorSpace::Xyz;
    for profile in [modern, xyz] {
        for direction in [
            TransformDirection::DeviceToPcs,
            TransformDirection::PcsToDevice,
        ] {
            check(
                &profile,
                RenderingIntent::Perceptual,
                direction,
                [0.25, 0.5, 0.75],
            );
        }
    }
}

#[test]
fn identity_lab_transforms_preserve_physical_units_across_table_encodings() {
    let input = [0.25f64, 0.5, 0.75];
    for version in [ProfileVersion::V2_1, ProfileVersion::V4_4] {
        for (source_kind, destination_kind, scale) in [
            (LutType::Lut8, LutType::Lut16, 65280. / 65535.),
            (LutType::Lut16, LutType::Lut8, 65535. / 65280.),
        ] {
            let mut source = profile(version, table(source_kind));
            let mut destination = profile(version, table(destination_kind));
            source.color_space = DataColorSpace::Lab;
            destination.color_space = DataColorSpace::Lab;
            let transform = source
                .create_transform_f64(
                    crate::Layout::Rgb,
                    &destination,
                    crate::Layout::Rgb,
                    TransformOptions {
                        rendering_intent: RenderingIntent::RelativeColorimetric,
                        prefer_fixed_point: false,
                        ..Default::default()
                    },
                )
                .unwrap();
            let mut output = [0.; 3];
            transform.transform(&input, &mut output).unwrap();
            for (actual, input) in output.into_iter().zip(input) {
                assert!(
                    (actual - input * scale).abs() < 2e-5,
                    "{version:?} {source_kind:?}->{destination_kind:?}: {actual} vs {}",
                    input * scale
                );
            }
        }
    }
}
