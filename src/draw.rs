use chrono::{NaiveDateTime, Datelike, NaiveDate, Timelike, Duration};
use image::{RgbImage, Rgb};
use crate::Res;
use font8x8::UnicodeFonts;
use std::convert::TryInto;


fn draw_text(buf: &mut RgbImage, color: Rgb<u8>, mut x_base: u32, y_orig: u32, text: &str) {
    for ch in text.chars() {
        let letter = font8x8::LATIN_FONTS.get(ch)
            .or_else(|| font8x8::BASIC_FONTS.get(ch))
            .unwrap_or_else(|| [255; 8]);
        {
            let mut y = y_orig;
            for row in &letter[..] {
                let mut x = x_base;
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
        x_base += 8;
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
    let one_day = Duration::days(1);

    let mut month_start = 0;
    let mut days = Vec::new();
    let mut last_day = Day {
        date: first.date() - one_day,
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
            let mut d = old_day.date + one_day;
            insert_day(old_day);
            while d < dt {
                insert_day(Day {
                    date: d,
                    points: [0; WIDTH]
                });
                d += one_day;
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
    pub fn into_image(self, timezone: i8) -> Res<RgbImage> {
        const OFFSET_X: u32 = 8*3 + 1;
        const OFFSET_Y: u32 = 8*2 + 1;
        const GRAY: [u8; 3] = [0x40, 0x40, 0x40];

        let (width, height) = (WIDTH as u32, self.days.len() as u32);
        let (width, height) = (width + OFFSET_X + 1, height + OFFSET_Y + 1);
        let mut img = RgbImage::from_raw(
            width, height,
            vec![0; (3*width*height) as usize]
        ).ok_or_else(|| format_err!("Unable to create RgbImage. Very strange"))?;

        let mut y = OFFSET_Y;
        for d in self.days {
            let mut x = OFFSET_X;
            for p in d.points.iter() {
                #[cfg(not(any(feature="normal", feature="comments", feature="posts")))]
                compile_error!("One and only one of features must be specified: normal, comments, posts");
                #[cfg(feature="normal")]
                let color = {
                    #[cfg(any(feature="comments", feature="posts"))]
                    compile_error!("One and only one of features must be specified: normal, comments, posts");
                    Rgb(match p {
                        // Step 1
                        0 => [0x00, 0x00, 0x00], // Black
                        1 => [0x00, 0xFF, 0x00], // Lime
                        2 => [0xFF, 0xFF, 0x00], // Yellow
                        3 => [0x00, 0xFF, 0xFF], // Cyan
                        4 => [0xFF, 0x00, 0x00],  // Red
                        5 => [0x3C, 0xB3, 0x71],  // DarkGreen
                        6 => [0x00, 0xFA, 0x9A], // MediumSpringGreen
                        7 => [0xAD, 0xFF, 0x2F], // GreenYellow
                        8 => [0xFF, 0xD7, 0x00], // Gold
                        9 => [0xFF, 0xFF, 0x00],  // Yellow
                        10 => [0xFF, 0xA5, 0x00], // Orange
                        11 => [0xFF, 0x7F, 0x50], // Coral
                        12 => [0xFA, 0x80, 0x72], // Salmon
                        13 => [0xDC, 0x14, 0x3C], // Crimson
                        14 => [0xFF, 0x14, 0x93], // Pink
                        15 => [0xFF, 0x00, 0xFF], // Magenta
                        16 => [0x8A, 0x2B, 0xE2], // BlueViolet
                        17 => [0x80, 0x00, 0x80], // Purple
                        // Fallback
                        18..=255 => [0xFF, 0xFF, 0xFF],  // White
                    })
                };
                #[cfg(feature="comments")]
                let color = {
                    #[cfg(any(feature="normal", feature="posts"))]
                    compile_error!("One and only one of features must be specified: normal, comments, posts");
                    Rgb(match p {
                        // Step 5
                        0 => [0x00, 0x00, 0x00], // Black
                        1..=5 => [0x00, 0xB3, 0x00], // 60% green
                        5..10 => [0x9A, 0x9A, 0x00], // 60% yellow
                        10..15 => [0x00, 0xFF, 0xFF], // Cyan
                        4..=9 => [0x7F, 0xFF, 0xD4], // Aquamarine
                        10..=15 => [0x3C, 0xB3, 0x71],  // DarkGreen
                        16..=21 => [0x00, 0xFA, 0x9A], // MediumSpringGreen
                        22..=27 => [0x00, 0xFF, 0x00], // Lime
                        28..=33 => [0xAD, 0xFF, 0x2F], // GreenYellow
                        34..=39 => [0xFF, 0xD7, 0x00], // Gold
                        40..=45 => [0xFF, 0xFF, 0x00],  // Yellow
                        46..=51 => [0xFF, 0xA5, 0x00], // Orange
                        52..=57 => [0xFF, 0x7F, 0x50], // Coral
                        58..=63 => [0xFA, 0x80, 0x72], // Salmon
                        64..=69 => [0xDC, 0x14, 0x3C], // Crimson
                        70..=75 => [0xFF, 0x00, 0x00], // Red
                        76..=81 => [0xFF, 0x14, 0x93], // Pink
                        82..=87 => [0xFF, 0x00, 0xFF], // Magenta
                        88..=93 => [0x8A, 0x2B, 0xE2], // BlueViolet
                        94..=99 => [0x80, 0x00, 0x80], // Purple
                        // Fallback
                        100..=255 => [0xFF, 0xFF, 0xFF],  // White
                    })
                };
                #[cfg(feature="posts")]
                let color = {
                    #[cfg(any(feature="normal", feature="comments"))]
                    compile_error!("One and only one of features must be specified: normal, comments, posts");
                    Rgb(match p {
                        // Step 1
                        0 => [0x00, 0x00, 0x00], // Black
                        1 => [0x00, 0xB3, 0x00], // 60% green
                        2 => [0x9A, 0x9A, 0x00], // 60% yellow
                        3 => [0x00, 0xFF, 0xFF], // Cyan
                        4 => [0x7F, 0xFF, 0xD4], // Aquamarine
                        5 => [0x3C, 0xB3, 0x71],  // DarkGreen
                        6 => [0x00, 0xFA, 0x9A], // MediumSpringGreen
                        7 => [0x00, 0xFF, 0x00], // Lime
                        // Step 2
                        8..=9 => [0xAD, 0xFF, 0x2F], // GreenYellow
                        10..=11 => [0xFF, 0xD7, 0x00], // Gold
                        12..=13 => [0xFF, 0xFF, 0x00],  // Yellow
                        14..=15 => [0xFF, 0xA5, 0x00], // Orange
                        16..=17 => [0xFF, 0x7F, 0x50], // Coral
                        18..=19 => [0xFA, 0x80, 0x72], // Salmon
                        20..=21 => [0xDC, 0x14, 0x3C], // Crimson
                        22..=23 => [0xFF, 0x00, 0x00], // Red
                        24..=25 => [0xFF, 0x14, 0x93], // Pink
                        26..=27 => [0xFF, 0x00, 0xFF], // Magenta
                        28..=29 => [0x8A, 0x2B, 0xE2], // BlueViolet
                        30..=31 => [0x80, 0x00, 0x80], // Purple
                        // Fallback
                        32..=255 => [0xFF, 0xFF, 0xFF],  // White
                    })
                };
                img.put_pixel(x, y, color);
                #[cfg(feature="pluses")]
                {
                    img.put_pixel(x+1, y, color);
                    img.put_pixel(x-1, y, color);
                    img.put_pixel(x-1, y+1, color);
                    img.put_pixel(x-1, y-1, color);
                }
                x += 1;
            }
            y += 1;
        }

        for month in self.months {
            let y = month.start as u32 + OFFSET_Y;
            for x in OFFSET_X..width {
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
                        match month.month {
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
                        }.to_string()
                    }
                );
            }
        }

        let timezone: u32 = (24i8 + timezone).try_into()?;
        for i in 0..=23 {
            let x = OFFSET_X + i*60;
            draw_text(
                &mut img,
                Rgb([255, 255, 255]),
                x, 0,
                &format!("{:02}:00", (i+timezone) % 24)
            );
            for y in OFFSET_Y..height {
                let px = img.get_pixel_mut(x, y);
                if px.0 == [0, 0, 0] {
                    *px = Rgb(GRAY);
                }
            }
        }

        Ok(img)
    }
}
