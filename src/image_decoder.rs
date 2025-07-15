use image::{GenericImage, Rgb, RgbImage};

struct PixelAverage {
    avg_rb: u32,
    avg_g: u32,
}

impl PixelAverage {
    pub fn new() -> Self {
        Self {
            avg_rb: 0,
            avg_g: 0,
        }
    }

    pub fn add(&mut self, rgb: u32) {
        self.avg_rb += rgb & 0x00FF00FF;
        self.avg_g += rgb & 0x0000FF00;
    }

    pub fn rgb(self) -> Rgb<u8> {
        let rb = (self.avg_rb / 16) as u8;
        let g = ((self.avg_g / 16) >> 8) as u8;
        Rgb([rb, g, rb])
    }
}

pub trait ToRgb {
    fn rgb(&self) -> Rgb<u8>;
}

pub struct RgbPixel {
    dat: [u8; 3],
}

impl RgbPixel {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { dat: [r, g, b] }
    }
}

impl ToRgb for RgbPixel {
    fn rgb(&self) -> Rgb<u8> {
        Rgb(self.dat)
    }
}

pub struct YUV420Pixel {
    dat: [u8; 3],
}

impl YUV420Pixel {
    pub fn new(c: u8, d: u8, e: u8) -> Self {
        Self { dat: [c, d, e] }
    }
}

fn clamp(v: i32) -> u8 {
    v.max(0).min(255) as u8
}

impl ToRgb for YUV420Pixel {
    fn rgb(&self) -> Rgb<u8> {
        let y = self.dat[0] as i32;
        let u = self.dat[1] as i32;
        let v = self.dat[2] as i32;
        let r = clamp(298 * (y - 16) + 409 * (v - 128) + 128 >> 8);
        let g = clamp(298 * (y - 16) - 100 * (u - 128) - 208 * (v - 128) + 128 >> 8);
        let b = clamp(298 * (y - 16) + 516 * (u - 128) + 128 >> 8);
        Rgb([r, g, b])
    }
}

pub struct Rgb565 {
    dat: u16,
}

impl Rgb565 {
    fn new(dat: u16) -> Self {
        Self { dat }
    }
}

impl ToRgb for Rgb565 {
    fn rgb(&self) -> Rgb<u8> {
        let r8 = ((self.dat >> 11) * 527 + 23) >> 6;
        let g8 = ((self.dat >> 5 & 0x3F) * 259 + 33) >> 6;
        let b8 = ((self.dat & 0x1F) * 527 + 23) >> 6;
        Rgb([r8 as u8, g8 as u8, b8 as u8])
    }
}

pub fn rgb565_to_rgb888(mapping: &[u16], pitch: u32, size: (u32, u32)) -> RgbImage {
    let mut img = RgbImage::new(size.0, size.1);
    let bytepitch = pitch / 2;

    for y in 0..size.1 {
        for x in 0..size.0 {
            let offset = (y * bytepitch + x) as usize;
            let v = Rgb565::new(mapping[offset]);
            unsafe { img.unsafe_put_pixel(x, y, v.rgb()) };
        }
    }
    img
}

pub fn decode_image(mapping: &[u32], pitch: u32, size: (u32, u32)) -> RgbImage {
    let mut img = RgbImage::new(size.0, size.1);
    let bytepitch = pitch / 4;

    for y in 0..size.1 {
        for x in 0..size.0 {
            let offset = (y * bytepitch + x) as usize;
            let v = mapping[offset];
            let px = Rgb([
                (v >> 16) as u8,
                (v >> 8) as u8,
                v as u8,
            ]);
            unsafe { img.unsafe_put_pixel(x, y, px) };
        }
    }
    img
}
pub fn decode_image_multichannel(
    mappings: [&[u8]; 3],
    size: (u32, u32),
    pitches: [u32; 3],
) -> RgbImage {
    let mut img = RgbImage::new(size.0, size.1);

    for y in 0..size.1 {
        for x in 0..size.0 {
            let offset = (y * pitches[0] + x) as usize;
            let offset1 = ((y / 2) * pitches[1] + x / 2) as usize;
            let offset2 = ((y / 2) * pitches[2] + x / 2) as usize;
            let yuv = YUV420Pixel::new(
                mappings[0][offset],
                mappings[1][offset1],
                mappings[2][offset2],
            );
            unsafe { img.unsafe_put_pixel(x, y, yuv.rgb()) };
        }
    }

    img
}

pub fn decode_small_image_multichannel(
    mappings: [&[u8]; 3],
    size: (u32, u32),
    pitches: [u32; 3],
) -> RgbImage {
    let halfsize = (size.0 / 2, size.1 / 2);
    let mut img = RgbImage::new(halfsize.0, halfsize.1);

    for y in 0..halfsize.1 {
        for x in 0..halfsize.0 {
            let offset = (2 * y * pitches[0] + 2 * x) as usize;
            let offset1 = (y * pitches[1] + x) as usize;
            let offset2 = (y * pitches[2] + x) as usize;
            let yval = (mappings[0][offset] as u32
                + mappings[0][offset + 1] as u32
                + mappings[0][offset + pitches[0] as usize] as u32
                + mappings[0][offset + pitches[0] as usize + 1] as u32)
                / 4;
            let yuv = YUV420Pixel::new(yval as u8, mappings[1][offset1], mappings[2][offset2]);
            unsafe { img.unsafe_put_pixel(x, y, yuv.rgb()) };
        }
    }

    img
}

pub fn decode_tiled_small_image(
    mapping: &[u32],
    tilesize: u32,
    tiles: (u32, u32),
    size: (u32, u32),
) -> RgbImage {
    let mut img = RgbImage::new(tiles.0 * tilesize / 4, tiles.1 * tilesize / 4);

    let mut i = 0;

    let mut avg_16 = |x, y| {
        let mut avg = PixelAverage::new();
        for n in 0..16 {
            avg.add(mapping[i + n]);
        }
        unsafe {
            img.unsafe_put_pixel(x, y, avg.rgb());
        }
        i = i + 16;
    };

    let mut copy_16x4_px = |x, y| {
        avg_16(x, y);
        avg_16(x + 1, y);
        avg_16(x + 2, y);
        avg_16(x + 3, y);
    };

    let mut copy_16x16_px = |x, y| {
        copy_16x4_px(x, y);
        copy_16x4_px(x, y + 1);
        copy_16x4_px(x, y + 2);
        copy_16x4_px(x, y + 3);
    };

    for ytile in 0..tiles.1 {
        if ytile % 2 == 0 {
            let mut copy_tile = |x, y| {
                copy_16x16_px(x, y);
                copy_16x16_px(x, y + 4);
                copy_16x16_px(x + 4, y + 4);
                copy_16x16_px(x + 4, y);
            };

            for xtile in 0..tiles.0 {
                copy_tile(xtile * tilesize / 4, ytile * tilesize / 4);
            }
        } else {
            let mut copy_tile = |x, y| {
                copy_16x16_px(x + 4, y + 4);
                copy_16x16_px(x + 4, y);
                copy_16x16_px(x, y);
                copy_16x16_px(x, y + 4);
            };

            for xtile in (0..tiles.0).rev() {
                copy_tile(xtile * tilesize / 4, ytile * tilesize / 4);
            }
        }
    }

    img.sub_image(0, 0, size.0 / 4, size.1 / 4).to_image()
}

pub fn to_image(mapping: &[u8], tilesize: u32, tiles: (u32, u32), size: (u32, u32)) -> RgbImage {
    let mut img = RgbImage::new(tiles.0 * tilesize, tiles.1 * tilesize);
    let mut i = 0;

    let mut copy_px = |x, y| {
        let color = Rgb([
            mapping[(i + 2) as usize],
            mapping[(i + 1) as usize],
            mapping[(i + 0) as usize],
        ]);
        unsafe {
            img.unsafe_put_pixel(x, y, color);
        }
        i = i + 4;
    };
    let mut copy_4_px = |x, y| {
        copy_px(x, y);
        copy_px(x + 1, y);
        copy_px(x + 2, y);
        copy_px(x + 3, y);
    };

    let mut copy_4x4_px = |x, y| {
        copy_4_px(x, y);
        copy_4_px(x, y + 1);
        copy_4_px(x, y + 2);
        copy_4_px(x, y + 3);
    };

    let mut copy_16x4_px = |x, y| {
        copy_4x4_px(x, y);
        copy_4x4_px(x + 4, y);
        copy_4x4_px(x + 8, y);
        copy_4x4_px(x + 12, y);
    };

    let mut copy_16x16_px = |x, y| {
        copy_16x4_px(x, y);
        copy_16x4_px(x, y + 4);
        copy_16x4_px(x, y + 8);
        copy_16x4_px(x, y + 12);
    };

    for ytile in 0..tiles.1 {
        if ytile % 2 == 0 {
            let mut copy_tile = |x, y| {
                copy_16x16_px(x, y);
                copy_16x16_px(x, y + 16);
                copy_16x16_px(x + 16, y + 16);
                copy_16x16_px(x + 16, y);
            };

            for xtile in 0..tiles.0 {
                copy_tile(xtile * tilesize, ytile * tilesize);
            }
        } else {
            let mut copy_tile = |x, y| {
                copy_16x16_px(x + 16, y + 16);
                copy_16x16_px(x + 16, y);
                copy_16x16_px(x, y);
                copy_16x16_px(x, y + 16);
            };

            for xtile in (0..tiles.0).rev() {
                copy_tile(xtile * tilesize, ytile * tilesize);
            }
        }
    }

    img.sub_image(0, 0, size.0, size.1).to_image()
}
