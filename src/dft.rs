// Spekje -- Spectrogram viewer
// Copyright 2019 Ruud van Asseldonk

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3. A copy
// of the License is available in the root of the repository.

// This module implements a fast fourier transform. At first I wanted to bind
// fftw, but its API is not that simple, and requires using their custom malloc
// and free. There are a few pure-rust options, but none of them with zero
// dependencies. So let's see how far I get on my own. It will not be the
// fastest, but perhaps it will be fast enough.

use std::f32::consts;

/// Apply a disrete Fourier Transform.
///
/// Returns the squared norms of the results. Only resturns the first half (+1)
/// of the coefficients, as they are symmetric.
pub fn dft_naive(xs: &[f32]) -> Box<[f32]> {
    let half_len = xs.len() / 2;
    assert_eq!(half_len * 2, xs.len(), "Length must be multiple of 2.");

    let mut result = Vec::with_capacity(half_len + 1);
    let inv_len = (xs.len() as f64).recip();

    for k in 0..=half_len {
        let factor = std::f64::consts::PI * 2.0 * k as f64 * inv_len;
        let mut real = 0.0_f64;
        let mut imag = 0.0_f64;
        for (n, x) in xs.iter().map(|&x| x as f64).enumerate() {
            real = x.mul_add((factor * n as f64).cos(), real);
            imag = x.mul_add((-factor * n as f64).sin(), imag);
        }
        result.push((real * real + imag * imag) as f32);
    }

    result.into_boxed_slice()
}

#[derive(Copy, Clone)]
struct Complex {
    real: f32,
    imag: f32,
}

impl Complex {
    pub fn mul_add(self, factor: Complex, term: Complex) -> Complex {
        Complex {
            real: self.real.mul_add(factor.real, term.real),
            imag: self.imag.mul_add(factor.imag, term.imag),
        }
    }
}

impl std::ops::Add for Complex {
    type Output = Complex;
    fn add(self, other: Complex) -> Complex {
        Complex {
            real: self.real + other.real,
            imag: self.imag + other.imag,
        }
    }
}

impl std::ops::Sub for Complex {
    type Output = Complex;
    fn sub(self, other: Complex) -> Complex {
        Complex {
            real: self.real - other.real,
            imag: self.imag - other.imag,
        }
    }
}

fn cooley_tukey(xs: &mut [Complex], tmp: &mut [Complex]) {
    if xs.len() < 2 { return }

    let half_len = xs.len() / 2;
    assert_eq!(half_len * 2, xs.len(), "Length must be even.");
    assert!(tmp.len() >= half_len);

    for i in 0..half_len {
        tmp[i] = xs[2 * i + 1];
        xs[i] = xs[2 * i];
    }
    for i in 0..half_len {
        xs[i + half_len] = tmp[i];
    }

    cooley_tukey(&mut xs[..half_len], tmp);
    cooley_tukey(&mut xs[half_len..], tmp);

    let inv_len = (xs.len() as f32).recip();
    let two_pi = 6.283185307179586;
    let two_pi_over_len = two_pi * inv_len;

    for i in 0..half_len {
        let arg = (i as f32) * two_pi_over_len;
        let cexp = Complex {
            real: -arg.cos(),
            imag: arg.sin(),
        };
        xs[i + half_len] = cexp.mul_add(xs[i + half_len], xs[i]);
        xs[i] = xs[i] + xs[i] - xs[i + half_len];
    }
}

pub fn dft_fast(xs: &[f32]) -> Box<[f32]> {
    let half_len = xs.len() / 2;
    assert_eq!(half_len * 2, xs.len(), "Length must be even.");

    let z = Complex {
        real: 0.0,
        imag: 0.0,
    };
    let mut tmp: Vec<_> = std::iter::repeat(z).take(half_len).collect();

    let mut xs_complex: Vec<_> = xs
        .iter()
        .map(|&x| Complex { real: x, imag: 0.0, })
        .collect();

    cooley_tukey(&mut xs_complex[..], &mut tmp[..]);

    let result: Vec<f32> = xs_complex
        .iter()
        .take(half_len + 1)
        .map(|&z| z.real * z.real + z.imag * z.imag)
        .collect();

    result.into_boxed_slice()
}

/// Build a signal which is a superposition of known waves.
///
/// Frequencies and amplitudes:
///
///     1.0 at 5.
///     2.0 at 31.
///     5.0 at 53.
///     7.0 at 541.
#[cfg(test)]
fn generate_test_signal() -> Box<[f32]> {
    let two_pi = 6.283185307179586;
    let buffer: Vec<f32> = (0..4096)
        .map(|i| {
            let t = i as f32 / 4096.0;
            0.0
            + 1.0 * (t *   5.0 * two_pi).sin()
            + 2.0 * (t *  31.0 * two_pi).cos()
            + 5.0 * (t *  53.0 * two_pi).sin()
            + 7.0 * (t * 541.0 * two_pi).sin()
        })
        .collect();

    buffer.into_boxed_slice()
}

#[test]
fn dft_naive_finds_peaks() {
    let buffer = generate_test_signal();
    let result_naive = dft_naive(&buffer[..]);
    let epsilon = 2e-4;

    for (i, &result) in result_naive.iter().enumerate() {
        let a = 2.0 * result.sqrt() / buffer.len() as f32;

        match i {
            5 => assert!((a - 1.0).abs() < epsilon),
            31 => assert!((a - 2.0).abs() < epsilon),
            53 => assert!((a - 5.0).abs() < epsilon),
            541 => assert!((a - 7.0).abs() < epsilon),
            _ => assert!(a < epsilon, "Unexpected peak of {} at {}", a, i),
        }
    }
}

#[test]
fn dft_fast_equals_dft_naive() {
    let buffer = generate_test_signal();
    let result_naive = dft_naive(&buffer[..]);
    let result_fast = dft_fast(&buffer[..]);

    for (i, (&naive, &fast)) in result_naive.iter().zip(result_fast.iter()).enumerate() {
        println!("{:4}: {:.2}  {:.2}", i, naive.sqrt(), fast.sqrt());
    }

    for (i, (&naive, &fast)) in result_naive.iter().zip(result_fast.iter()).enumerate() {
        let diff = (naive / (fast + 1e-10)).max(fast / (naive + 1e-10));
        assert!(diff < 1.0001, "Difference at index {}: {} vs {}.", i, naive, fast);
        assert!(diff > 0.9999, "Difference at index {}: {} vs {}.", i, naive, fast);
    }
}
