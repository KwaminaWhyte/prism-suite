use super::*;

// ---- Smart Filters (non-destructive filter stack) ------------------------

// A smart Gaussian blur softens a hard edge, and *disabling* it (re-applying an
// empty pass list, then clearing the source) restores the original pixels
// exactly — proving the stack is non-destructive (the source is never baked).

#[test]
fn smart_gaussian_blur_softens_then_disabling_restores_original() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping smart_gaussian_blur_softens_then_disabling_restores_original");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 16u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    // Hard vertical black/white edge at x = n/2.
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, _y| {
            if x < n / 2 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }),
    );
    let row = n / 2;
    // The two pixels straddling the edge start fully black / fully white.
    let pre_l = gpu.read_pixel(&device, &queue, l0, n / 2 - 1, row).unwrap()[0];
    let pre_r = gpu.read_pixel(&device, &queue, l0, n / 2, row).unwrap()[0];
    assert!(pre_l < 0.02 && pre_r > 0.98, "hard edge before blur: {pre_l} {pre_r}");

    // Apply a smart Gaussian blur (shader kind 1) over the source. This snapshots
    // the un-filtered source and writes the blurred result into the layer.
    gpu.reapply_smart_filters(&device, &queue, l0, &[crate::SmartPass::scalar(1, 6.0, 0.0)]);
    assert!(gpu.has_smart_source(l0), "blur snapshotted the source");
    let blur_l = gpu.read_pixel(&device, &queue, l0, n / 2 - 1, row).unwrap()[0];
    let blur_r = gpu.read_pixel(&device, &queue, l0, n / 2, row).unwrap()[0];
    // The edge is now soft: the dark side brightened, the bright side darkened,
    // both into intermediate values (a true blend, never the hard 0/1 step).
    assert!(blur_l > 0.05, "dark side softened up: {blur_l}");
    assert!(blur_r < 0.95, "bright side softened down: {blur_r}");
    assert!(blur_l < blur_r, "still brighter on the right: {blur_l} {blur_r}");

    // Re-applying with the filter *disabled* (empty pass list) resets to source.
    gpu.reapply_smart_filters(&device, &queue, l0, &[]);
    let off_l = gpu.read_pixel(&device, &queue, l0, n / 2 - 1, row).unwrap()[0];
    let off_r = gpu.read_pixel(&device, &queue, l0, n / 2, row).unwrap()[0];
    assert!(
        off_l < 0.02 && off_r > 0.98,
        "disabling restores the hard edge: {off_l} {off_r}"
    );

    // Clearing the source (last filter removed) leaves the original pixels and
    // drops the snapshot — the layer is plain, destructively-editable again.
    gpu.clear_smart_source(&device, &queue, l0);
    assert!(!gpu.has_smart_source(l0), "source dropped after clear");
    for x in 0..n {
        let v = gpu.read_pixel(&device, &queue, l0, x, row).unwrap()[0];
        let want = if x < n / 2 { 0.0 } else { 1.0 };
        assert!((v - want).abs() < 0.02, "original restored at x={x}: {v}");
    }
}


// The smart-filter source is captured once and never re-snapshotted: editing the
// stack (here, swapping a blur radius) still re-applies from the pristine source,
// so a strong blur followed by a zero-radius blur returns the hard edge.
#[test]
fn smart_filter_edit_reapplies_from_pristine_source() {
    let Some((device, queue)) = device() else {
        eprintln!("no GPU adapter; skipping smart_filter_edit_reapplies_from_pristine_source");
        return;
    };
    let mut gpu = CanvasGpu::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let n = 16u32;
    gpu.ensure_canvas(&device, Size::new(n, n));
    let l0 = LayerId(0);
    gpu.ensure_layer(&device, l0);
    gpu.upload_layer(
        &queue,
        l0,
        &layer_from(n, |x, _y| {
            if x < n / 2 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }),
    );
    let row = n / 2;
    // Strong blur, then edit the same entry down to radius 0 (a no-op pass).
    gpu.reapply_smart_filters(&device, &queue, l0, &[crate::SmartPass::scalar(1, 8.0, 0.0)]);
    gpu.reapply_smart_filters(&device, &queue, l0, &[crate::SmartPass::scalar(1, 0.0, 0.0)]);
    // Because the source was never overwritten, the hard edge is back exactly.
    for x in 0..n {
        let v = gpu.read_pixel(&device, &queue, l0, x, row).unwrap()[0];
        let want = if x < n / 2 { 0.0 } else { 1.0 };
        assert!(
            (v - want).abs() < 0.03,
            "edit re-applies from pristine source at x={x}: {v}"
        );
    }
}
