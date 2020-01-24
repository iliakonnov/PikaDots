use chrono::{NaiveDateTime, Datelike, NaiveDate, Timelike, Duration};
use image::{RgbImage, Rgb};
use crate::Res;
use font8x8::UnicodeFonts;


fn draw_text(buf: &mut RgbImage, color: Rgb<u8>, mut x: u32, y: u32, text: &str) {
    for ch in text.chars() {
        let letter = font8x8::LATIN_FONTS.get(ch)
            .or(font8x8::BASIC_FONTS.get(ch))
            .unwrap_or_else(|| [255; 8]);
        {
            let mut y = y;
            for row in &letter[..] {
                let mut x = x;
                for bit in 0..8 {
                    match *row & (1 << bit) {
                        0 => {},
                        _ => buf.put_pixel(x, y, color)
                    }
                    x += 1;
                }
                y += 1;
            }
        }
        x += 8;
    }
}

const WIDTH: usize = 60*24;

struct Day {
    date: NaiveDate,
    points: [u8; WIDTH]
}

#[derive(Debug, Clone, Copy)]
struct Month {
    start: usize,
    end: usize,
    month: u8,
    year: i32
}

pub struct Generated {
    days: Vec<Day>,
    months: Vec<Month>
}

// TODO: Generate merged image of all users
// This will use a lot of memory, so it requires somehow append data to existing Generated

pub fn generate(points: &[NaiveDateTime]) -> Generated {
    if points.is_empty() {
        return Generated {
            days: Vec::new(),
            months: Vec::new()
        }
    }
    let first = points[0];
    let ONE_DAY = Duration::days(1);

    let mut month_start = 0;
    let mut days = Vec::new();
    let mut last_day = Day {
        date: first.date() - ONE_DAY,
        points: [0; WIDTH]
    };
    let mut months = Vec::new();

    for p in points {
        assert!(p.date() >= last_day.date);
        let dt = p.date();
        if dt != last_day.date {
            assert!(p.date() > last_day.date);
            let old_day = std::mem::replace(&mut last_day, Day {
                date: dt,
                points: [0; WIDTH]
            });
            let mut insert_day = |d: Day| {
                if d.date.day() == 1 {
                    // Start of month
                    let idx = days.len();
                    months.push(Month {
                        start: month_start,
                        end: idx - 1,  // Before `d` in `days`
                        month: match d.date.month() {  // Previous month
                            1 => 12,
                            x => x-1
                        } as u8,
                        year: d.date.year()
                    });
                    month_start = idx;  // Index of `d` in `days`
                }
                days.push(d);
            };
            let mut d = old_day.date + ONE_DAY;
            insert_day(old_day);
            while d < dt {
                insert_day(Day {
                    date: d,
                    points: [0; WIDTH]
                });
                d += ONE_DAY;
            }
        }
        let position = (p.hour()*60 + p.minute()) as usize;
        debug_assert!(position < 60*24);
        //unsafe{std::intrinsics::assume(position < 60*24)};
        last_day.points[position] += 1;
    }

    Generated {
        days,
        months
    }
}

impl Generated {
    // TODO: Optimize and remove second pass. But it is very bad idea
    pub fn into_image(self) -> Res<RgbImage> {
        const OFFSET_X: u32 = 8*3;
        const OFFSET_Y: u32 = 8*2;
        const GRAY: [u8; 3] = [0x40, 0x40, 0x40];

        let (width, height) = (WIDTH as u32, self.days.len() as u32);
        let (width, height) = (width + OFFSET_X, height + OFFSET_Y);
        let mut img = RgbImage::from_raw(
            width, height,
            vec![0; (3*width*height) as usize]
        ).ok_or_else(|| format_err!("Unable to create RgbImage. Very strange"))?;

        let mut y = OFFSET_Y;
        for d in self.days {
            let mut x = OFFSET_X;
            for (i, p) in d.points.iter().enumerate() {
                let px = img.get_pixel_mut(x, y);
                *px = Rgb(match p {
                    0 => [0x00, 0x00, 0x00], // Black
                    // Black -> Blue (step 1)
                    1 => [0x00, 0x00, 0xFF], // Blue
                    2 => [0x1E, 0x90, 0xFF], // DodgerBlue
                    3 => [0x00, 0xFF, 0xFF], // Cyan
                    4 => [0x7F, 0xFF, 0xD4], // Aquamarine
                    // Blue -> Green (step 2)
                    5..=7 => [0x3C, 0xB3, 0x71],  // DarkGreen
                    8..=10 => [0x00, 0xFA, 0x9A], // MediumSpringGreen
                    11..=13 => [0x00, 0xFF, 0x00], // Lime
                    13..=15 => [0xAD, 0xFF, 0x2F], // GreenYellow
                    // Green -> Yellow (step 6)
                    16..=21 => [0xFF, 0xD7, 0x00], // Gold
                    22..=27 => [0xFF, 0xFF, 0x00],  // Yellow
                    28..=33 => [0xFF, 0xA5, 0x00], // Orange
                    34..=39 => [0xFF, 0x7F, 0x50], // Coral
                    // Orange -> Red (step 12)
                    40..=51 => [0xFA, 0x80, 0x72], // Salmon
                    52..=63 => [0xDC, 0x14, 0x3C], // Crimson
                    64..=75 => [0xFF, 0x00, 0x00], // Red
                    // Red -> Pink (step 24)
                    78..=101 => [0xFF, 0x14, 0x93], // Pink
                    102..=125 => [0xFF, 0x00, 0xFF], // Magenta
                    126..=149 => [0x8A, 0x2B, 0xE2], // BlueViolet
                    150..=173 => [0x80, 0x00, 0x80], // Purple
                    // Fallback
                    _ => [0xFF, 0xFF, 0xFF],  // White
                });
                x += 1;
            }
            y += 1;
        }

        for month in self.months {
            let y = month.start as u32 + OFFSET_Y;
            for x in (OFFSET_X..width) {
                let px = img.get_pixel_mut(x, y);
                if px.0 == [0, 0, 0] {
                    *px = Rgb(GRAY);
                }
            }

            let y = month.end as u32 + OFFSET_Y;
            if y >= 15 {
                let is_january = month.month == 1;
                draw_text(
                    &mut img,
                    Rgb(if is_january { [255, 0, 255] } else { [255, 255, 255] }),
                    0, y-15,
                    &if is_january { format!("'{:02}", month.year % 100) } else {
                        format!("{}", match month.month {
                            1 => "Jan",
                            2 => "Feb",
                            3 => "Mar",
                            4 => "Apr",
                            5 => "May",
                            6 => "Jun",
                            7 => "Jul",
                            8 => "Aug",
                            9 => "Sep",
                            10 => "Oct",
                            11 => "Nov",
                            12 => "Dec",
                            _ => "???"
                        })
                    }
                );
            }
        }

        for (i, x) in (OFFSET_X..width).step_by(60).enumerate() {
            draw_text(
                &mut img,
                Rgb([255, 255, 255]),
                x, 0,
                &format!("{:02}:00", i)
            );
            for y in (OFFSET_Y..height) {
                let px = img.get_pixel_mut(x, y);
                if px.0 == [0, 0, 0] {
                    *px = Rgb(GRAY);
                }
            }
        }

        Ok(img)
    }
}
