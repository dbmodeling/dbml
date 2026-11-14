#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Diagram export without UI dependencies: Mermaid `erDiagram`, vector SVG and PNG/JPEG
//! rasterisation (ADR-0007: scene → SVG → raster).

mod mermaid;
mod raster;
mod svg;

pub use mermaid::to_mermaid;
pub use raster::{RasterError, svg_to_jpeg, svg_to_png};
pub use svg::{Theme, to_svg};
