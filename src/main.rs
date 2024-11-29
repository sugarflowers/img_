use image::{self, RgbImage};
use serde::{Deserialize, Serialize};
use std::{fs, error::Error};
use clipboard::{ClipboardProvider, ClipboardContext};


#[derive(Debug, Deserialize, Serialize)]
struct Colors {
    palette: Vec<[u8; 3]>,
}
#[derive(Debug, Deserialize)]
struct Offsets {
    offsets: Vec<(i32, i32, f64)>,
}

fn read_offsets(offsets_path: &str) -> Result<Vec<(i32, i32, f64)>, Box<dyn Error>> {
    let toml_str = fs::read_to_string(offsets_path)?;
    let offsets: Offsets = toml::from_str(&toml_str)?;
    let mapped_offsets: Vec<(i32, i32, f64)> = offsets.offsets.iter()
        .map(|&(x, y, magnification)| (x as i32, y as i32, magnification as f64))
        .collect();
    Ok(mapped_offsets)
}


fn read_palette(palette_path: &str) -> Result<Vec<(i32, i32, i32)>, Box<dyn Error>> {
    let toml_str = fs::read_to_string(palette_path)?;
    let colors: Colors = toml::from_str(&toml_str)?;
    let mapped_colors: Vec<(i32, i32, i32)> = colors.palette.iter()
        .map(|&[r, g, b]| (r as i32, g as i32, b as i32))
        .collect();
    Ok(mapped_colors)
}

pub fn set_clipboard(text: &str) {
    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
    ctx.set_contents(text.to_owned()).unwrap();
}

fn rgb_to_hsv((r, g, b): (i32, i32, i32)) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };

    let s = if max == 0.0 {
        0.0
    } else {
        delta / max
    };

    (h, s, max)
}


#[derive(Default, Debug)]
pub struct Converter {
    pub image_org: RgbImage,
    pub image_converted: RgbImage,
    pub palette: Vec<(i32, i32, i32)>,
    pub offsets: Vec<(i32, i32, f64)>,
    pub width: u32,
    pub height: u32,

}

impl Converter {
    pub fn new() -> Converter {
        let p = match read_palette("def/palette.toml") {
            Ok(colors) => colors,
            Err(e) => {
                println!("error by readeing palette file : {}", e);
                vec![(0, 0, 0)]
            }
        };
        let o = match read_offsets("def/offset.toml") {
            Ok(ofs) => ofs,
            Err(e) => {
                println!("error by reading offset file : {}", e);
                vec![(0, 0, 0.0)]
            }
        };

        let con = Converter {
            palette: p,
            offsets: o,
            ..Converter::default()
        };
        con
    }

    pub fn read_image(mut self, file_path: &str) -> Self {
        let img = image::open(file_path).unwrap();
        self.image_org = img.to_rgb8();
        self.width = img.width();
        self.height = img.height();

        self
    }

    fn userdata(&self) {
    
        let mut buf = "".to_string();
        for y in 0..self.height {
            for x in 0..self.width {
                let pix = self.image_converted.get_pixel(x, y);
                let r = pix[0] as i32;
                let g = pix[1] as i32;
                let b = pix[2] as i32;
                buf = format!("{}{:02x}", buf, self.find_closest_palette_index((r, g, b)));
            }
        }

        //println!("{:?}", buf);
        set_clipboard(&format!("userdata(\"u8\", {}, {}, \"{}\")", self.width, self.height, buf));
    }

    fn find_closest_palette_index(&self, pixel: (i32, i32, i32)) -> usize {
        self.palette.iter()
            .enumerate()
            .min_by_key(|&(_, &(pr, pg, pb))| {
                let dr = pixel.0 - pr;
                let dg = pixel.1 - pg;
                let db = pixel.2 - pb;
                (dr * dr + dg * dg + db * db) as i64
            })
            .unwrap()
            .0
    }

    fn find_closest_palette_color(&self, pixel: (i32, i32, i32)) -> &(i32, i32, i32) {
        let min_distance = self.palette.iter()
            .map(|&(pr, pg, pb)| {
                let dr = (pixel.0 - pr) as i64;
                let dg = (pixel.1 - pg) as i64;
                let db = (pixel.2 - pb) as i64;
                (dr * dr + dg * dg + db * db) as i64
            })
            .min()
            .unwrap();

        let mut candidates: Vec<&(i32, i32, i32)> = self.palette.iter()
            .filter(|&&(pr, pg, pb)| {
                let dr = (pixel.0 - pr) as i64;
                let dg = (pixel.1 - pg) as i64;
                let db = (pixel.2 - pb) as i64;
                (dr * dr + dg * dg + db * db) as i64 == min_distance
            })
            .collect();

        if candidates.len() == 1 {
            return candidates[0];
        }

        candidates.sort_by_key(|&&(pr, pg, pb)| {
            let (h_pixel, s_pixel, v_pixel) = rgb_to_hsv(pixel);
            let (h_palette, s_palette, v_palette) = rgb_to_hsv((pr, pg, pb));
            let dh = (h_pixel - h_palette).abs() as i64;
            let ds = (s_pixel - s_palette).abs() as i64;
            let dv = (v_pixel - v_palette).abs() as i64;
            dh * dh + ds * ds + dv * dv
        });

        candidates[0]
    }

    fn idx(&self, x:u32, y:u32) -> usize {
        (y * self.width + x) as usize
    }

    pub fn error_diffusion(mut self) -> Self {
        // make buffer
        let mut r_buf: Vec<i32> = Vec::new();
        let mut g_buf: Vec<i32> = Vec::new();
        let mut b_buf: Vec<i32> = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let pix = self.image_org.get_pixel(x, y);
                r_buf.push(pix[0] as i32);
                g_buf.push(pix[1] as i32);
                b_buf.push(pix[2] as i32);
            }
        }
        // working
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = self.idx(x, y);
                let old_pixel = (r_buf[idx], g_buf[idx], b_buf[idx]);
                let new_pixel = self.find_closest_palette_color(old_pixel);
                let error = (
                    old_pixel.0 - new_pixel.0, 
                    old_pixel.1 - new_pixel.1, 
                    old_pixel.2 - new_pixel.2
                );
                r_buf[idx] = new_pixel.0;
                g_buf[idx] = new_pixel.1;
                b_buf[idx] = new_pixel.2;

                for &(dx, dy, factor) in &self.offsets {
                    let nx = (x as i32 + dx) as u32;
                    let ny = (y as i32 + dy) as u32;
                    if nx > 0 && nx < self.width-1 && ny > 0 && ny < self.height-1 {
                        let idx = self.idx(nx, ny);
                        r_buf[idx] += (error.0 as f64 * factor) as i32;
                        g_buf[idx] += (error.1 as f64 * factor) as i32;
                        b_buf[idx] += (error.2 as f64 * factor) as i32;
                    }
                }
            }
        }
        // create new image
        self.image_converted = RgbImage::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = self.idx(x, y);
                let r = r_buf[idx] as u8;
                let g = g_buf[idx] as u8;
                let b = b_buf[idx] as u8;
                self.image_converted.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }

        self
    }


    pub fn bayer(mut self) -> Self {
        let bayer:Vec<u8> = vec![ 
             0, 32,  8, 40,  2, 34, 10, 42,  48, 16, 56, 24, 50, 18, 58, 26,
            12, 44,  4, 36, 14, 46,  6, 38,  60, 28, 52, 20, 62, 30, 54, 22,
             3, 35, 11, 43,  1, 33,  9, 41,  51, 19, 59, 27, 49, 17, 57, 25,
            15, 47,  7, 39, 13, 45,  5, 37,  63, 31, 55, 23, 61, 29, 53, 21];

        let bayer_idx = |x:u32, y:u32| -> usize {((y % 8) * 8 + (x % 8)).try_into().unwrap()};
        let rng = |v:i32| -> i32 { v.clamp(0, 255) };

        // make buffer
        let mut r_buf: Vec<i32> = Vec::new();
        let mut g_buf: Vec<i32> = Vec::new();
        let mut b_buf: Vec<i32> = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let pix = self.image_org.get_pixel(x, y);
                r_buf.push(pix[0] as i32);
                g_buf.push(pix[1] as i32);
                b_buf.push(pix[2] as i32);
            }
        }

        for y in 0..self.height {
            for x in 0..self.width {
                let by = bayer[bayer_idx(x, y)] as i32;
                let idx = self.idx(x, y);
                let r = r_buf[idx] as i32;
                let g = g_buf[idx] as i32;
                let b = b_buf[idx] as i32; 
                let r = rng(r + by - 32);
                let g = rng(g + by - 32);
                let b = rng(b + by - 32);
                let (r, g, b) = self.find_closest_palette_color((r, g, b)); 

                r_buf[idx] = *r;
                g_buf[idx] = *g;
                b_buf[idx] = *b;
            }
        }

        self.image_converted = RgbImage::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = self.idx(x, y);
                let r = r_buf[idx] as u8;
                let g = g_buf[idx] as u8;
                let b = b_buf[idx] as u8;
                self.image_converted.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }

        self
    }

    pub fn save(&self, save_file_path: &str) {
        self.image_converted.save(save_file_path).unwrap();    
    }
}



const IMAGE_FILE: &str = "onarimon.jpg";

fn main() {

    Converter::new().read_image(IMAGE_FILE)
        .error_diffusion()
        //.save("output.png");
        .userdata();

    /*
    Converter::new().read_image(IMAGE_FILE)
        .bayer()
        .save("bayer.png");
    */

}





