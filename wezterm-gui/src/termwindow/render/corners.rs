use crate::customglyph::*;

pub const TOP_LEFT_ROUNDED_CORNER: &[Poly] = &[Poly {
    path: &[PolyCommand::Oval {
        center: (BlockCoord::One, BlockCoord::One),
        radiuses: (BlockCoord::One, BlockCoord::One),
    }],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

pub const BOTTOM_LEFT_ROUNDED_CORNER: &[Poly] = &[Poly {
    path: &[PolyCommand::Oval {
        center: (BlockCoord::One, BlockCoord::Zero),
        radiuses: (BlockCoord::One, BlockCoord::One),
    }],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

pub const TOP_RIGHT_ROUNDED_CORNER: &[Poly] = &[Poly {
    path: &[PolyCommand::Oval {
        center: (BlockCoord::Zero, BlockCoord::One),
        radiuses: (BlockCoord::One, BlockCoord::One),
    }],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

pub const BOTTOM_RIGHT_ROUNDED_CORNER: &[Poly] = &[Poly {
    path: &[PolyCommand::Oval {
        center: (BlockCoord::Zero, BlockCoord::Zero),
        radiuses: (BlockCoord::One, BlockCoord::One),
    }],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

pub const TOP_LEFT_SLANT_CORNER: &[Poly] = &[Poly {
    path: &[
        PolyCommand::MoveTo(BlockCoord::One, BlockCoord::Zero),
        PolyCommand::LineTo(BlockCoord::One, BlockCoord::One),
        PolyCommand::LineTo(BlockCoord::Zero, BlockCoord::One),
        PolyCommand::Close,
    ],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

pub const BOTTOM_RIGHT_SLANT_CORNER: &[Poly] = &[Poly {
    path: &[
        PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Zero),
        PolyCommand::LineTo(BlockCoord::One, BlockCoord::Zero),
        PolyCommand::LineTo(BlockCoord::Zero, BlockCoord::One),
        PolyCommand::Close,
    ],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

/// The point of a tab group's popup that sits above its item, aimed down.
pub const TAB_GROUP_POINT_DOWN: &[Poly] = &[Poly {
    path: &[
        PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Zero),
        PolyCommand::LineTo(BlockCoord::One, BlockCoord::Zero),
        PolyCommand::LineTo(BlockCoord::Frac(1, 2), BlockCoord::One),
        PolyCommand::Close,
    ],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];

/// The point of a tab group's popup that sits below its item, aimed up.
pub const TAB_GROUP_POINT_UP: &[Poly] = &[Poly {
    path: &[
        PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::One),
        PolyCommand::LineTo(BlockCoord::One, BlockCoord::One),
        PolyCommand::LineTo(BlockCoord::Frac(1, 2), BlockCoord::Zero),
        PolyCommand::Close,
    ],
    intensity: BlockAlpha::Full,
    style: PolyStyle::Fill,
}];
