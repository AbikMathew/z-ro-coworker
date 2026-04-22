//! Ring buffer holding the last N screen frames captured by the SCK stream.
//!
//! Why a ring buffer, not a single "latest" slot?
//! - The validator needs "before" and "after" frames to judge whether a step
//!   was completed. Keeping ~5 frames means the validator always has a recent
//!   "before" without racing the capture stream.
//! - We want cheap reads across tasks. Storing `bytes::Bytes` (not `Vec<u8>`)
//!   lets `.clone()` share the same allocation at zero cost.
//! - Capacity is bounded so a long session can't leak memory — the oldest
//!   frame is evicted on every push.
//!
//! Why `std::sync::Mutex` (synchronous) rather than a tokio RwLock? The SCK
//! stream handler runs on an Apple dispatch queue, not a tokio executor — it
//! can't `.await`. Keeping the lock synchronous means the handler can push a
//! frame without needing a tokio context. Contention is negligible: the ring
//! holds at most 5 entries and push/read take microseconds.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;

/// Maximum number of frames kept in the ring. Anthropic Computer Use examples
/// typically work with ~5 screenshots of context; more than that and we pay
/// for tokens+memory without accuracy gain.
pub const FRAME_CAPACITY: usize = 5;

/// One captured screen frame plus the metadata needed to interpret its pixels.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Raw pixel bytes. Format is BGRA8 (native macOS SCK output). Stored as
    /// `Bytes` so multiple consumers can clone a read-only view for free.
    pub pixels: Bytes,
    /// Pixel width of the captured source (window / display).
    pub width: u32,
    /// Pixel height of the captured source.
    pub height: u32,
    /// Bytes per row in `pixels` — SCK may pad rows for alignment, so this
    /// can be larger than `width * 4`. Callers decoding BGRA must respect it.
    pub bytes_per_row: u32,
    /// Backing scale factor at capture time. Needed to translate point-based
    /// AX coordinates to pixel coordinates on retina displays.
    pub scale_factor: f32,
    /// Monotonic clock timestamp of when the frame landed in the buffer.
    pub captured_at: Instant,
}

impl Frame {
    /// Encode the BGRA pixels as a JPEG byte vec. Respects `bytes_per_row`
    /// so SCK's row padding doesn't produce garbage rows.
    pub fn to_jpeg(&self, quality: u8) -> Result<Vec<u8>, String> {
        use image::{ImageBuffer, Rgba};
        let packed_row = (self.width * 4) as usize;
        let row_stride = self.bytes_per_row as usize;
        let mut rgba = Vec::with_capacity((self.width * self.height * 4) as usize);
        for y in 0..self.height as usize {
            let start = y * row_stride;
            let end = start + packed_row;
            if end > self.pixels.len() {
                return Err(format!(
                    "Frame::to_jpeg: truncated row {y} (stride={row_stride}, pixels={})",
                    self.pixels.len()
                ));
            }
            for chunk in self.pixels[start..end].chunks_exact(4) {
                rgba.push(chunk[2]); // R
                rgba.push(chunk[1]); // G
                rgba.push(chunk[0]); // B
                rgba.push(chunk[3]); // A
            }
        }
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(self.width, self.height, rgba)
                .ok_or_else(|| "Frame::to_jpeg: ImageBuffer::from_raw failed".to_string())?;

        let mut out = Vec::new();
        let mut encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        encoder
            .encode_image(&img)
            .map_err(|e| format!("Frame::to_jpeg: {e}"))?;
        Ok(out)
    }

    /// Convenience: JPEG encode then base64-encode. Used by the LLM prompt
    /// builder which passes images as base64 data URIs.
    pub fn to_jpeg_base64(&self, quality: u8) -> Result<String, String> {
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(self.to_jpeg(quality)?))
    }
}

/// Thread-safe ring of frames. Cheap to clone — internally an `Arc` to the
/// same inner queue.
#[derive(Clone)]
pub struct FrameBuffer {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    frames: VecDeque<Arc<Frame>>,
    capacity: usize,
}

impl FrameBuffer {
    pub fn new() -> Self {
        Self::with_capacity(FRAME_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                frames: VecDeque::with_capacity(capacity),
                capacity,
            })),
        }
    }

    /// Push a new frame. Evicts the oldest when the ring is full. On a
    /// poisoned lock (panicking thread) we silently drop the frame — better
    /// than crashing the capture stream.
    pub fn push(&self, frame: Frame) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if guard.frames.len() >= guard.capacity {
            guard.frames.pop_front();
        }
        guard.frames.push_back(Arc::new(frame));
    }

    /// Most recent frame, if any. `Arc` makes this a cheap clone.
    pub fn latest(&self) -> Option<Arc<Frame>> {
        let guard = self.inner.lock().ok()?;
        guard.frames.back().cloned()
    }

    /// Oldest frame still in the ring (the "before" context for verification).
    pub fn oldest(&self) -> Option<Arc<Frame>> {
        let guard = self.inner.lock().ok()?;
        guard.frames.front().cloned()
    }

    /// Snapshot of all frames in insertion order.
    pub fn snapshot(&self) -> Vec<Arc<Frame>> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        guard.frames.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.frames.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every frame. Called when a capture session ends so stale pixels
    /// don't linger.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.frames.clear();
        }
    }
}

impl Default for FrameBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_frame(tag: u8) -> Frame {
        Frame {
            pixels: Bytes::copy_from_slice(&[tag; 16]),
            width: 2,
            height: 2,
            bytes_per_row: 8,
            scale_factor: 1.0,
            captured_at: Instant::now(),
        }
    }

    #[test]
    fn push_and_latest_returns_most_recent() {
        let buf = FrameBuffer::new();
        buf.push(test_frame(1));
        buf.push(test_frame(2));
        let latest = buf.latest().unwrap();
        assert_eq!(latest.pixels[0], 2);
    }

    #[test]
    fn push_evicts_oldest_when_full() {
        let buf = FrameBuffer::with_capacity(3);
        for i in 0..5u8 {
            buf.push(test_frame(i));
        }
        assert_eq!(buf.len(), 3);
        let oldest = buf.oldest().unwrap();
        let latest = buf.latest().unwrap();
        assert_eq!(oldest.pixels[0], 2); // 0 and 1 were evicted
        assert_eq!(latest.pixels[0], 4);
    }

    #[test]
    fn clear_empties_the_ring() {
        let buf = FrameBuffer::new();
        buf.push(test_frame(1));
        buf.push(test_frame(2));
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.latest().is_none());
    }

    #[test]
    fn snapshot_preserves_insertion_order() {
        let buf = FrameBuffer::with_capacity(4);
        for i in 0..3u8 {
            buf.push(test_frame(i));
        }
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].pixels[0], 0);
        assert_eq!(snap[1].pixels[0], 1);
        assert_eq!(snap[2].pixels[0], 2);
    }

    #[test]
    fn cloning_the_buffer_shares_state() {
        let a = FrameBuffer::with_capacity(3);
        let b = a.clone();
        a.push(test_frame(7));
        assert_eq!(b.len(), 1);
        assert_eq!(b.latest().unwrap().pixels[0], 7);
    }
}
