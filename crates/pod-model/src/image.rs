//! Imagem entregue a uma view (ADR-0001, D3: `Image`): pixels BGRA com alfa pré-multiplicado, já
//! reduzidos pelo host. O widget nunca decodifica nada nem desenha: só passa a imagem que um serviço
//! (por exemplo a capa do álbum) entregou. Limites validados em `View::validate`.

use std::sync::Arc;

/// Tamanho máximo, em bytes, de uma imagem (D3: 512 KiB).
pub const MAX_IMAGE_BYTES: usize = 512 * 1024;
/// Quantas imagens uma view pode ter (D3).
pub const MAX_IMAGES: usize = 4;

#[derive(Debug, Clone)]
pub struct ImageData {
    /// Identifica o conteúdo: duas imagens com o mesmo `id` são tratadas como iguais (diff de view).
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, B G R A com alfa pré-multiplicado.
    pub bgra: Vec<u8>,
}

impl PartialEq for ImageData {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.width == o.width && self.height == o.height
    }
}

impl ImageData {
    /// Imagem válida: dimensões positivas, tamanho coerente com os pixels e dentro do limite.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.bgra.len() == self.width as usize * self.height as usize * 4
            && self.bgra.len() <= MAX_IMAGE_BYTES
    }

    /// Cor média dos pixels (amostra em grade), em RGB; cinza médio se a imagem for inválida.
    pub fn average_rgb(&self) -> [u8; 3] {
        if !self.is_valid() {
            return [0x80, 0x80, 0x80];
        }
        let step = ((self.width.max(self.height)) / 24).max(1) as usize;
        let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
        for y in (0..self.height as usize).step_by(step) {
            for x in (0..self.width as usize).step_by(step) {
                let i = (y * self.width as usize + x) * 4;
                b += self.bgra[i] as u64;
                g += self.bgra[i + 1] as u64;
                r += self.bgra[i + 2] as u64;
                n += 1;
            }
        }
        let n = n.max(1);
        [(r / n) as u8, (g / n) as u8, (b / n) as u8]
    }

    /// Imagem de teste/demonstração: gradiente diagonal entre duas cores, com `id` derivado delas.
    pub fn gradient(width: u32, height: u32, from: [u8; 3], to: [u8; 3]) -> Arc<Self> {
        let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let t = (x + y) as f32 / (width + height).max(1) as f32;
                let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
                bgra.extend_from_slice(&[mix(from[2], to[2]), mix(from[1], to[1]), mix(from[0], to[0]), 255]);
            }
        }
        let id = (from[0] as u64) << 40 | (from[1] as u64) << 32 | (from[2] as u64) << 24 | (to[0] as u64) << 16 | (to[1] as u64) << 8 | to[2] as u64;
        Arc::new(Self { id, width, height, bgra })
    }
}

/// Resumo FNV-1a dos pixels: serve de `id` para imagens vindas de fora.
pub fn content_id(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Menor tamanho que cabe em `max_side` mantendo a proporção; nunca amplia e nunca devolve zero.
pub fn fit_within(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= max_side || longest == 0 {
        return (width.max(1), height.max(1));
    }
    let scale = max_side as f64 / longest as f64;
    (((width as f64 * scale).round() as u32).max(1), ((height as f64 * scale).round() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_is_valid_and_equality_uses_the_id() {
        let a = ImageData::gradient(8, 8, [10, 20, 30], [200, 100, 50]);
        assert!(a.is_valid());
        assert_eq!(a.bgra.len(), 8 * 8 * 4);
        let b = ImageData::gradient(8, 8, [10, 20, 30], [200, 100, 50]);
        assert_eq!(a, b, "mesmo conteúdo, mesmo id");
        let c = ImageData::gradient(8, 8, [11, 20, 30], [200, 100, 50]);
        assert_ne!(a, c);
        // Alfa opaco e primeiro pixel = cor inicial (B, G, R, A).
        assert_eq!(&a.bgra[0..4], &[30, 20, 10, 255]);
    }

    #[test]
    fn invalid_images_are_rejected() {
        let ok = ImageData { id: 1, width: 2, height: 2, bgra: vec![0; 16] };
        assert!(ok.is_valid());
        assert!(!ImageData { bgra: vec![0; 15], ..ok.clone() }.is_valid(), "tamanho incoerente");
        assert!(!ImageData { width: 0, bgra: vec![], ..ok.clone() }.is_valid(), "largura zero");
        let big = ImageData { id: 2, width: 400, height: 400, bgra: vec![0; 400 * 400 * 4] };
        assert!(!big.is_valid(), "640 KB passa de 512 KiB");
    }

    #[test]
    fn content_id_changes_with_the_bytes() {
        assert_eq!(content_id(b"abc"), content_id(b"abc"));
        assert_ne!(content_id(b"abc"), content_id(b"abd"));
        assert_ne!(content_id(b""), content_id(b"a"));
    }

    #[test]
    fn fit_within_keeps_the_ratio_and_never_upscales() {
        assert_eq!(fit_within(640, 320, 160), (160, 80));
        assert_eq!(fit_within(300, 600, 150), (75, 150));
        assert_eq!(fit_within(100, 50, 160), (100, 50), "já cabe");
        assert_eq!(fit_within(1000, 1, 100), (100, 1), "nunca zero");
        assert_eq!(fit_within(0, 0, 100), (1, 1));
    }
}
