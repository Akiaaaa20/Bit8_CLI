use serde::Serialize;
use std::fmt::{Display, Formatter};

/// Fixed Camera viewport dimensions in game pixels. Camera `x` and `y` identify its center.
pub const CAMERA_WIDTH: i64 = 64;
pub const CAMERA_HEIGHT: i64 = 64;
pub const CAMERA_HALF_WIDTH: i64 = CAMERA_WIDTH / 2;
pub const CAMERA_HALF_HEIGHT: i64 = CAMERA_HEIGHT / 2;

/// Stable identifiers for the built-in Node types supported by Bit8.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
pub enum NodeType {
    #[default]
    #[serde(rename = "Node")]
    Node,
    #[serde(rename = "Camera")]
    Camera,
}

/// The complete built-in type set. This is intentionally static and not extensible at runtime.
pub const BUILT_IN_NODE_TYPES: &[NodeType] = &[NodeType::Node, NodeType::Camera];

impl NodeType {
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Node => "Node",
            Self::Camera => "Camera",
        }
    }

    pub fn from_identifier(identifier: &str) -> Option<Self> {
        BUILT_IN_NODE_TYPES
            .iter()
            .copied()
            .find(|node_type| node_type.identifier() == identifier)
    }
}

impl Display for NodeType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.identifier())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_viewport_is_fixed_at_the_framebuffer_size() {
        assert_eq!((CAMERA_WIDTH, CAMERA_HEIGHT), (64, 64));
        assert_eq!(
            (CAMERA_HALF_WIDTH, CAMERA_HALF_HEIGHT),
            (CAMERA_WIDTH / 2, CAMERA_HEIGHT / 2)
        );
    }
}
