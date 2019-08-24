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
pub fn dft(xs: &[f32]) -> Box<[f32]> {
    let half_len = xs.len() / 2;
    assert_eq!(half_len * 2, xs.len(), "Length must be multiple of 2.");

    let mut result = Vec::with_capacity(half_len + 1);
    let inv_len = (xs.len() as f32).recip();

    for k in 0..=half_len {
        let factor = consts::PI * 2.0 * k as f32 * inv_len;
        let mut real = 0.0_f32;
        let mut imag = 0.0_f32;
        for (n, &x) in xs.iter().enumerate() {
            real = x.mul_add((factor * n as f32).cos(), real);
            imag = x.mul_add((-factor * n as f32).sin(), imag);
        }
        result.push(real * real + imag * imag);
    }

    result.into_boxed_slice()
}
