//! Fingerprints of a screen region, to tell just before a click on an image
//! target that the screen there still shows what was detected (REQ-ACT-03).

/// Columns of the fingerprint grid; text regions are wide and short.
pub const COLUMNS: usize = 16;
/// Rows of the fingerprint grid.
pub const ROWS: usize = 4;
/// Cells in a fingerprint.
pub const CELLS: usize = COLUMNS * ROWS;

/// The average brightness of each grid cell over a region of an RGBA image,
/// `crop` being x, y, width and height in the image's pixels, clipped to it.
pub fn fingerprint(
    rgba: &[u8],
    width: u32,
    height: u32,
    crop: (u32, u32, u32, u32),
) -> [u8; CELLS] {
    let (x0, y0, w, h) = crop;
    let x1 = x0.saturating_add(w).min(width);
    let y1 = y0.saturating_add(h).min(height);
    let mut cells = [0u8; CELLS];
    if x0 >= x1 || y0 >= y1 {
        return cells;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut sums = [0u64; CELLS];
    let mut counts = [0u64; CELLS];
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y as usize) * (width as usize) + x as usize) * 4;
            let Some(px) = rgba.get(i..i + 3) else {
                continue;
            };
            let luma =
                (299 * u64::from(px[0]) + 587 * u64::from(px[1]) + 114 * u64::from(px[2])) / 1000;
            let column = (x - x0) as usize * COLUMNS / w;
            let row = (y - y0) as usize * ROWS / h;
            sums[row * COLUMNS + column] += luma;
            counts[row * COLUMNS + column] += 1;
        }
    }
    for (cell, (sum, count)) in cells.iter_mut().zip(sums.iter().zip(counts)) {
        *cell = sum.checked_div(count).unwrap_or(0) as u8;
    }
    cells
}

/// Whether two fingerprints show markedly different content: their cells
/// differ by more than ten levels on average, which text scrolled into the
/// region does and a blinking cursor in one cell does not.
pub fn differ(then: &[u8; CELLS], now: &[u8; CELLS]) -> bool {
    let total: usize = then
        .iter()
        .zip(now)
        .map(|(a, b)| usize::from(a.abs_diff(*b)))
        .sum();
    total > 10 * CELLS
}
