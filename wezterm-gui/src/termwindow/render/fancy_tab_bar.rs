use crate::customglyph::*;
use crate::tabbar::{TabBarItem, TabEntry};
use crate::termwindow::box_model::*;
use crate::termwindow::render::corners::*;

use crate::termwindow::render::window_buttons::window_button_element;
use crate::termwindow::tabgrouppopup;
use crate::termwindow::{UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::{
    Dimension, DimensionContext, FancyTabBarStyle, TabBarColors, TabMarker, TabShape,
    TabVerticalAlign, TitleBackgroundPaint,
};
use std::borrow::Cow;
use std::rc::Rc;
use wezterm_font::LoadedFont;
use wezterm_term::color::{ColorAttribute, ColorPalette};
use wezterm_term::Line;
use window::color::LinearRgba;
use window::{IntegratedTitleButtonAlignment, IntegratedTitleButtonStyle};

/// Every tab has a border this wide, painted like its background except
/// for the edge of an inactive tab.
const TAB_BORDER: f32 = 1.;

/// The title as its tab draws it. Each run of a title paints its own
/// background, so a background the tab must not show is cleared from the
/// cells themselves: moved to their text for `Text`, dropped where `shown`
/// is false.
fn paint_title(title: &Line, paint: TitleBackgroundPaint, shown: bool) -> Cow<'_, Line> {
    let has_background = || {
        title
            .visible_cells()
            .any(|cell| cell.attrs().background() != ColorAttribute::Default)
    };
    if (shown && paint == TitleBackgroundPaint::Fill) || !has_background() {
        return Cow::Borrowed(title);
    }
    let mut title = title.clone();
    for cell in title.cells_mut_for_attr_changes_only() {
        let background = cell.attrs().background();
        if background == ColorAttribute::Default {
            continue;
        }
        let attrs = cell.attrs_mut();
        if shown {
            attrs.set_foreground(background);
        }
        attrs.set_background(ColorAttribute::Default);
    }
    Cow::Owned(title)
}

fn pixel_box(p: PixelDimension) -> BoxDimension {
    BoxDimension {
        left: Dimension::Pixels(p.left),
        right: Dimension::Pixels(p.right),
        top: Dimension::Pixels(p.top),
        bottom: Dimension::Pixels(p.bottom),
    }
}

/// The tab geometry `fancy_tab_bar_style` asks for, resolved against the
/// title font once per tab bar build.
struct TabLook {
    style: FancyTabBarStyle,
    is_bottom: bool,
    padding_x: f32,
    padding_y: f32,
    corner_x: f32,
    /// The border box height of an unmarked tab.
    height: f32,
    marker_width: f32,
}

impl TabLook {
    fn new(
        style: &FancyTabBarStyle,
        is_bottom: bool,
        width: DimensionContext,
        height: DimensionContext,
    ) -> Self {
        let padding_y = style.tab_padding_y.evaluate_as_pixels(height);
        Self {
            style: style.clone(),
            is_bottom,
            padding_x: style.tab_padding_x.evaluate_as_pixels(width),
            padding_y,
            corner_x: style.tab_corner_size.evaluate_as_pixels(width),
            height: height.pixel_cell + 2. * (padding_y + TAB_BORDER),
            marker_width: style.active_tab_marker_width.evaluate_as_pixels(height),
        }
    }

    fn vertical_align(&self) -> VerticalAlign {
        match self.style.tab_vertical_align {
            TabVerticalAlign::Middle => VerticalAlign::Middle,
            TabVerticalAlign::TerminalEdge if self.is_bottom => VerticalAlign::Top,
            TabVerticalAlign::TerminalEdge => VerticalAlign::Bottom,
        }
    }

    /// In pixels, so the padding drawn is the padding `marker_split` divides.
    fn padding(&self) -> BoxDimension {
        pixel_box(PixelDimension {
            left: self.padding_x,
            right: self.padding_x,
            top: self.padding_y,
            bottom: self.padding_y,
        })
    }

    fn margin(&self) -> BoxDimension {
        let far_edge = match self.style.tab_vertical_align {
            TabVerticalAlign::TerminalEdge => Dimension::Cells(0.2),
            TabVerticalAlign::Middle => Dimension::Cells(0.),
        };
        BoxDimension {
            left: Dimension::Cells(0.),
            right: self.style.tab_gap,
            top: if self.is_bottom {
                Dimension::Cells(0.)
            } else {
                far_edge
            },
            bottom: if self.is_bottom {
                far_edge
            } else {
                Dimension::Cells(0.)
            },
        }
    }

    /// The corners of a tab whose border box is `height` pixels tall.
    fn corners(&self, active: bool, height: f32) -> Option<Corners> {
        match self.style.tab_shape {
            TabShape::Square => None,
            TabShape::Rounded => {
                let size = self.style.tab_corner_size;
                let round = |poly| SizedPoly {
                    width: size,
                    height: size,
                    poly,
                };
                let near = if active {
                    SizedPoly::none()
                } else {
                    SizedPoly {
                        width: Dimension::Cells(0.),
                        height: Dimension::Cells(0.33),
                        poly: &[],
                    }
                };
                Some(if self.is_bottom {
                    Corners {
                        top_left: near,
                        top_right: near,
                        bottom_left: round(BOTTOM_LEFT_ROUNDED_CORNER),
                        bottom_right: round(BOTTOM_RIGHT_ROUNDED_CORNER),
                    }
                } else {
                    Corners {
                        top_left: round(TOP_LEFT_ROUNDED_CORNER),
                        top_right: round(TOP_RIGHT_ROUNDED_CORNER),
                        bottom_left: near,
                        bottom_right: near,
                    }
                })
            }
            TabShape::Pill => {
                let radius = Dimension::Pixels(height / 2.);
                let round = |poly| SizedPoly {
                    width: radius,
                    height: radius,
                    poly,
                };
                Some(Corners {
                    top_left: round(TOP_LEFT_ROUNDED_CORNER),
                    top_right: round(TOP_RIGHT_ROUNDED_CORNER),
                    bottom_left: round(BOTTOM_LEFT_ROUNDED_CORNER),
                    bottom_right: round(BOTTOM_RIGHT_ROUNDED_CORNER),
                })
            }
            TabShape::Slant => {
                let run = Dimension::Pixels(self.corner_x);
                let cut = |poly| SizedPoly {
                    width: run,
                    height: Dimension::Pixels(height),
                    poly,
                };
                // The background between the cut corners covers the full
                // height only when the uncut corners claim the same width.
                let uncut = SizedPoly {
                    width: run,
                    height: Dimension::Pixels(0.),
                    poly: &[],
                };
                Some(Corners {
                    top_left: cut(TOP_LEFT_SLANT_CORNER),
                    top_right: uncut,
                    bottom_left: uncut,
                    bottom_right: cut(BOTTOM_RIGHT_SLANT_CORNER),
                })
            }
        }
    }

    /// The padding of the marker around the active tab and of the tab
    /// inside it, or None when the tab's colors are its only marker.
    fn marker_split(&self) -> Option<(PixelDimension, PixelDimension)> {
        let w = self.marker_width;
        let outer = match self.style.active_tab_marker {
            TabMarker::Fill => return None,
            TabMarker::TopLine => PixelDimension {
                top: w,
                ..Default::default()
            },
            TabMarker::BottomLine => PixelDimension {
                bottom: w,
                ..Default::default()
            },
            TabMarker::Ring => PixelDimension {
                left: w,
                right: w,
                top: w,
                bottom: w,
            },
        };
        Some((outer, self.inner_of(&outer)))
    }

    /// The tab's own padding inside a marker of width `outer`.
    fn inner_of(&self, outer: &PixelDimension) -> PixelDimension {
        PixelDimension {
            left: (self.padding_x - outer.left).max(0.),
            right: (self.padding_x - outer.right).max(0.),
            top: (self.padding_y - outer.top).max(0.),
            bottom: (self.padding_y - outer.bottom).max(0.),
        }
    }

    /// Puts the active tab inside an element of the marker color that
    /// shows around it on the marker's sides.
    fn mark(&self, tab: Element, font: &Rc<LoadedFont>, color: LinearRgba) -> Element {
        match self.marker_split() {
            Some((outer, inner)) => self.surround(tab, font, color, outer, inner),
            None => tab,
        }
    }

    /// Puts a tab inside a ring of `color`, whatever marker the style gives
    /// the active tab, so it shows in a style whose active tab is marked by
    /// its fill alone.
    fn ring(&self, tab: Element, font: &Rc<LoadedFont>, color: LinearRgba) -> Element {
        let w = self.marker_width.max(2.);
        let outer = PixelDimension {
            left: w,
            right: w,
            top: w,
            bottom: w,
        };
        let inner = self.inner_of(&outer);
        self.surround(tab, font, color, outer, inner)
    }

    fn surround(
        &self,
        mut tab: Element,
        font: &Rc<LoadedFont>,
        color: LinearRgba,
        outer: PixelDimension,
        inner: PixelDimension,
    ) -> Element {
        // One hover region per tab: two would make crossing the marker a
        // hover change, and each hover change rebuilds every tab title.
        let item_type = tab.item_type.take();
        let tab = tab
            .margin(BoxDimension::new(Dimension::Pixels(0.)))
            .padding(pixel_box(inner))
            .border_corners(self.corners(true, self.height - outer.top - outer.bottom))
            .vertical_align(VerticalAlign::Top);
        let mut marked = Element::new(font, ElementContent::Children(vec![tab]))
            .vertical_align(self.vertical_align())
            .margin(self.margin())
            .padding(pixel_box(outer))
            .border_corners(self.corners(true, self.height))
            .colors(ElementColors {
                border: BorderColor::new(color),
                bg: color.into(),
                text: InheritableColor::Inherited,
            });
        marked.item_type = item_type;
        marked
    }

    /// A folded tab group: its item with two layers showing past its trailing
    /// edge, the nearest first, so it reads as a stack of tabs. The layers take
    /// the tab's own corners, so they follow the shape the style draws.
    fn stack(
        &self,
        mut tab: Element,
        font: &Rc<LoadedFont>,
        active: bool,
        layers: [LinearRgba; 2],
    ) -> Element {
        let item_type = tab.item_type.take();
        let mut stacked = tab.margin(BoxDimension::new(Dimension::Pixels(0.)));
        for color in layers {
            stacked = Element::new(font, ElementContent::Children(vec![stacked]))
                .vertical_align(VerticalAlign::Top)
                .padding(BoxDimension {
                    left: Dimension::Pixels(0.),
                    right: Dimension::Cells(STACK_STEP),
                    top: Dimension::Pixels(0.),
                    bottom: Dimension::Pixels(0.),
                })
                .border_corners(self.corners(active, self.height))
                .colors(ElementColors {
                    border: BorderColor::new(color),
                    bg: color.into(),
                    text: InheritableColor::Inherited,
                });
        }
        stacked = stacked
            .vertical_align(self.vertical_align())
            .margin(self.margin());
        stacked.item_type = item_type;
        stacked
    }
}

/// How far each layer of a folded tab group shows past the one in front, in
/// cells of the tab bar font.
const STACK_STEP: f32 = 0.3;

impl TabLook {
    /// The corners of the popup a folded group opens, a box `height` pixels
    /// tall: the tab's own shape, on all four corners where a rounded tab
    /// rounds only the two away from the terminal.
    fn popup_corners(&self, height: f32) -> Option<Corners> {
        match self.style.tab_shape {
            TabShape::Rounded => {
                let size = self.style.tab_corner_size;
                let round = |poly| SizedPoly {
                    width: size,
                    height: size,
                    poly,
                };
                Some(Corners {
                    top_left: round(TOP_LEFT_ROUNDED_CORNER),
                    top_right: round(TOP_RIGHT_ROUNDED_CORNER),
                    bottom_left: round(BOTTOM_LEFT_ROUNDED_CORNER),
                    bottom_right: round(BOTTOM_RIGHT_ROUNDED_CORNER),
                })
            }
            _ => self.corners(true, height),
        }
    }
}

const X_BUTTON: &[Poly] = &[
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::One, BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::Zero, BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::One, BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
];

const PLUS_BUTTON: &[Poly] = &[
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Frac(1, 2), BlockCoord::Zero),
            PolyCommand::LineTo(BlockCoord::Frac(1, 2), BlockCoord::One),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
    Poly {
        path: &[
            PolyCommand::MoveTo(BlockCoord::Zero, BlockCoord::Frac(1, 2)),
            PolyCommand::LineTo(BlockCoord::One, BlockCoord::Frac(1, 2)),
        ],
        intensity: BlockAlpha::Full,
        style: PolyStyle::Outline,
    },
];

/// How far a dimmed tab's title stays from the tab bar's background while
/// tab group hints are shown: enough that the folded groups stand out, not so
/// little that the other titles cannot be read.
const HINT_DIM: f32 = 0.35;

/// The color of a dimmed tab's title: the inactive tabs' text color, moved
/// toward the tab bar's background until `HINT_DIM` of it is left.
fn dim_color(colors: &TabBarColors) -> ColorAttribute {
    let fg: termwiz::color::SrgbaTuple = colors.inactive_tab().fg_color.into();
    let bg: termwiz::color::SrgbaTuple = colors.background().into();
    let mix = |f: f32, b: f32| b + (f - b) * HINT_DIM;
    ColorAttribute::TrueColorWithDefaultFallback(termwiz::color::SrgbaTuple(
        mix(fg.0, bg.0),
        mix(fg.1, bg.1),
        mix(fg.2, bg.2),
        1.0,
    ))
}

/// A folded group's title with `hint` where its tab numbers were: the
/// title's first `prefix_cells` cells, its leading space and numbers
/// (`tab_group_prefix`), become a space, the hint in the hint colors and a
/// colon. The tab bar's font may be proportional, so the item keeps its
/// width by being given the unhinted title's width, not by cell count.
fn hinted_title(
    title: &Line,
    prefix_cells: usize,
    hint: char,
    fg: ColorAttribute,
    bg: ColorAttribute,
) -> Line {
    let lead = title
        .get_cell(0)
        .map(|cell| cell.attrs().clone())
        .unwrap_or_default();
    let mut hint_attrs = lead.clone();
    hint_attrs
        .set_foreground(fg)
        .set_background(bg)
        .set_intensity(termwiz::cell::Intensity::Bold);
    let len = title.len();
    let prefix = prefix_cells.clamp(1, len.max(1));
    let seq = termwiz::surface::SEQ_ZERO;
    let mut line = Line::from_text(" ", &lead, seq, None);
    line.append_line(
        Line::from_text(&hint.to_string(), &hint_attrs, seq, None),
        seq,
    );
    line.append_line(Line::from_text(": ", &lead, seq, None), seq);
    line.append_line(title.columns_as_line(prefix..len), seq);
    line
}

/// A tab's title in `dim`, without the background its own title gives it.
fn dimmed_title(title: &Line, dim: ColorAttribute) -> Line {
    let mut out = title.clone();
    for cell in out.cells_mut_for_attr_changes_only() {
        let attrs = cell.attrs_mut();
        attrs.set_foreground(dim);
        attrs.set_background(ColorAttribute::Default);
    }
    out
}

impl crate::TermWindow {
    pub fn invalidate_fancy_tab_bar(&mut self) {
        self.fancy_tab_bar.take();
    }

    /// The tab bar and the open tab group popup laid out against it, if one
    /// is open.
    pub fn build_fancy_tab_bar(
        &self,
        palette: &ColorPalette,
    ) -> anyhow::Result<(ComputedElement, Option<Vec<ComputedElement>>)> {
        let tab_bar_height = self.tab_bar_pixel_height()?;
        let font = self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());
        let items = self.tab_bar.items();
        let colors = self
            .config
            .colors
            .as_ref()
            .and_then(|c| c.tab_bar.as_ref())
            .cloned()
            .unwrap_or_else(TabBarColors::default);

        // While the tab group hints are shown, each folded group's item reads
        // its hint where its tab numbers were, and every other tab is dimmed.
        // A group's title as it was is kept beside it, so its item can be
        // given that title's width and nothing in the bar moves.
        let hinted_items;
        let mut unhinted: Vec<Option<Line>> = vec![];
        let items = if self.tab_group_hints_open() {
            let folded = self.tab_bar.folded();
            let hints = tabgrouppopup::hints(&self.config.quick_select_alphabet, folded.len());
            let (hint_fg, hint_bg) = tabgrouppopup::hint_attributes(&self.config);
            let dim = dim_color(&colors);
            hinted_items = items
                .iter()
                .map(|item| {
                    let mut item = item.clone();
                    let mut before = None;
                    match item.item {
                        TabBarItem::TabGroup { group_idx, .. } => {
                            if let (Some(&hint), Some(group)) =
                                (hints.get(group_idx), folded.get(group_idx))
                            {
                                let prefix = crate::tabbar::tab_group_prefix(
                                    &group.tabs,
                                    self.config.show_tab_index_in_tab_bar,
                                    self.config.tab_and_split_indices_are_zero_based,
                                );
                                before = Some(item.title.clone());
                                item.title = hinted_title(
                                    &item.title,
                                    termwiz::cell::unicode_column_width(&prefix, None),
                                    hint,
                                    hint_fg,
                                    hint_bg,
                                );
                            }
                        }
                        TabBarItem::Tab { .. } => item.title = dimmed_title(&item.title, dim),
                        _ => {}
                    }
                    unhinted.push(before);
                    item
                })
                .collect::<Vec<TabEntry>>();
            &hinted_items[..]
        } else {
            items
        };

        let mut left_status = vec![];
        let mut left_eles = vec![];
        let mut right_eles = vec![];
        let bar_colors = ElementColors {
            border: BorderColor::default(),
            bg: if self.focused.is_some() {
                self.config.window_frame.active_titlebar_bg
            } else {
                self.config.window_frame.inactive_titlebar_bg
            }
            .to_linear()
            .into(),
            text: if self.focused.is_some() {
                self.config.window_frame.active_titlebar_fg
            } else {
                self.config.window_frame.inactive_titlebar_fg
            }
            .to_linear()
            .into(),
        };
        let width_context = DimensionContext {
            dpi: self.dimensions.dpi as f32,
            pixel_max: self.dimensions.pixel_width as f32,
            pixel_cell: metrics.cell_size.width as f32,
        };
        let height_context = DimensionContext {
            dpi: self.dimensions.dpi as f32,
            pixel_max: self.dimensions.pixel_height as f32,
            pixel_cell: metrics.cell_size.height as f32,
        };
        let style = &self.config.fancy_tab_bar_style;
        let look = TabLook::new(
            style,
            self.config.tab_bar_at_bottom,
            width_context,
            height_context,
        );
        let marker_color = style
            .active_tab_marker_color
            .unwrap_or_else(|| colors.active_tab().fg_color)
            .to_linear();

        let item_to_elem = |item: &TabEntry| -> Element {
            let title = match item.item {
                TabBarItem::Tab { active, .. } | TabBarItem::TabGroup { active, .. } => {
                    paint_title(
                        &item.title,
                        style.title_background_paints,
                        !active || style.active_tab_title_background,
                    )
                }
                _ => Cow::Borrowed(&item.title),
            };
            let element = Element::with_line(&font, &title, palette);

            let bg_color = title
                .get_cell(0)
                .and_then(|c| match c.attrs().background() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_bg(col)),
                });
            let fg_color = title
                .get_cell(0)
                .and_then(|c| match c.attrs().foreground() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_fg(col)),
                });

            let new_tab = colors.new_tab();
            let new_tab_hover = colors.new_tab_hover();
            let active_tab = colors.active_tab();

            match item.item {
                TabBarItem::RightStatus | TabBarItem::LeftStatus | TabBarItem::None => element
                    .item_type(UIItemType::TabBar(TabBarItem::None))
                    .line_height(Some(style.row_height as f64))
                    .margin(BoxDimension {
                        left: Dimension::Cells(0.),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.0),
                        bottom: Dimension::Cells(0.),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.),
                        top: Dimension::Cells(0.),
                        bottom: Dimension::Cells(0.),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(0.)))
                    .colors(bar_colors.clone()),
                TabBarItem::NewTabButton => Element::new(
                    &font,
                    ElementContent::Poly {
                        line_width: metrics.underline_height.max(2),
                        poly: SizedPoly {
                            poly: PLUS_BUTTON,
                            width: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                            height: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                        },
                    },
                )
                .vertical_align(VerticalAlign::Middle)
                .item_type(UIItemType::TabBar(item.item.clone()))
                .margin(BoxDimension {
                    left: Dimension::Cells(0.5),
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.2),
                    bottom: Dimension::Cells(0.),
                })
                .padding(BoxDimension {
                    left: Dimension::Cells(0.5),
                    right: Dimension::Cells(0.5),
                    top: Dimension::Cells(0.2),
                    bottom: Dimension::Cells(0.25),
                })
                .border(BoxDimension::new(Dimension::Pixels(1.)))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: new_tab.bg_color.to_linear().into(),
                    text: new_tab.fg_color.to_linear().into(),
                })
                .hover_colors(Some(ElementColors {
                    border: BorderColor::default(),
                    bg: new_tab_hover.bg_color.to_linear().into(),
                    text: new_tab_hover.fg_color.to_linear().into(),
                })),
                TabBarItem::Tab { active: true, .. }
                | TabBarItem::TabGroup { active: true, .. } => {
                    let bg = bg_color
                        .unwrap_or_else(|| active_tab.bg_color.into())
                        .to_linear();
                    element
                        .vertical_align(look.vertical_align())
                        .item_type(UIItemType::TabBar(item.item.clone()))
                        .margin(look.margin())
                        .padding(look.padding())
                        .border(BoxDimension::new(Dimension::Pixels(TAB_BORDER)))
                        .border_corners(look.corners(true, look.height))
                        .colors(ElementColors {
                            border: BorderColor::new(bg),
                            bg: bg.into(),
                            text: fg_color
                                .unwrap_or_else(|| active_tab.fg_color.into())
                                .to_linear()
                                .into(),
                        })
                }
                TabBarItem::Tab { .. } | TabBarItem::TabGroup { .. } => element
                    .vertical_align(look.vertical_align())
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(look.margin())
                    .padding(look.padding())
                    .border(BoxDimension::new(Dimension::Pixels(TAB_BORDER)))
                    .border_corners(look.corners(false, look.height))
                    .colors({
                        let inactive_tab = colors.inactive_tab();
                        let bg = bg_color
                            .unwrap_or_else(|| inactive_tab.bg_color.into())
                            .to_linear();
                        let edge = colors.inactive_tab_edge().to_linear();
                        ElementColors {
                            border: BorderColor {
                                left: bg,
                                right: edge,
                                top: bg,
                                bottom: bg,
                            },
                            bg: bg.into(),
                            text: fg_color
                                .unwrap_or_else(|| inactive_tab.fg_color.into())
                                .to_linear()
                                .into(),
                        }
                    })
                    .hover_colors({
                        let inactive_tab_hover = colors.inactive_tab_hover();
                        Some(ElementColors {
                            border: BorderColor::new(
                                bg_color
                                    .unwrap_or_else(|| inactive_tab_hover.bg_color.into())
                                    .to_linear(),
                            ),
                            bg: bg_color
                                .unwrap_or_else(|| inactive_tab_hover.bg_color.into())
                                .to_linear()
                                .into(),
                            text: fg_color
                                .unwrap_or_else(|| inactive_tab_hover.fg_color.into())
                                .to_linear()
                                .into(),
                        })
                    }),
                TabBarItem::WindowButton(button) => window_button_element(
                    button,
                    self.window_state.contains(window::WindowState::MAXIMIZED),
                    &font,
                    &metrics,
                    &self.config,
                ),
            }
        };

        let num_tabs: f32 = items
            .iter()
            .map(|item| match item.item {
                TabBarItem::NewTabButton | TabBarItem::Tab { .. } | TabBarItem::TabGroup { .. } => {
                    1.
                }
                _ => 0.,
            })
            .sum();
        let max_tab_width = ((self.dimensions.pixel_width as f32 / num_tabs)
            - (1.5 * metrics.cell_size.width as f32))
            .max(0.);

        // Reserve space for the native titlebar buttons
        if self
            .config
            .window_decorations
            .contains(::window::WindowDecorations::INTEGRATED_BUTTONS)
            && self.config.integrated_title_button_style == IntegratedTitleButtonStyle::MacOsNative
            && !self.window_state.contains(window::WindowState::FULL_SCREEN)
        {
            left_status.push(
                Element::new(&font, ElementContent::Text("".to_string())).margin(BoxDimension {
                    left: Dimension::Cells(4.0), // FIXME: determine exact width of macos ... buttons
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                }),
            );
        }

        let stack_layers = {
            let edge = colors.inactive_tab_edge().to_linear();
            [edge, edge.mul_alpha(0.55)]
        };

        for (item_idx, item) in items.iter().enumerate() {
            match item.item {
                TabBarItem::LeftStatus => left_status.push(item_to_elem(item)),
                TabBarItem::None | TabBarItem::RightStatus => right_eles.push(item_to_elem(item)),
                TabBarItem::WindowButton(_) => {
                    if self.config.integrated_title_button_alignment
                        == IntegratedTitleButtonAlignment::Left
                    {
                        left_eles.push(item_to_elem(item))
                    } else {
                        right_eles.push(item_to_elem(item))
                    }
                }
                TabBarItem::Tab { .. } | TabBarItem::TabGroup { .. } => {
                    let (tab_idx, active, is_group) = match item.item {
                        TabBarItem::Tab { tab_idx, active } => (tab_idx, active, false),
                        // A group's tab index would only number a close
                        // button, which a group does not have.
                        TabBarItem::TabGroup { active, .. } => (0, active, true),
                        _ => unreachable!(),
                    };
                    let mut elem = item_to_elem(item);
                    if let Some(Some(before)) = unhinted.get(item_idx) {
                        let measured = self.compute_element(
                            &LayoutContext {
                                height: height_context,
                                width: width_context,
                                bounds: euclid::rect(
                                    0.,
                                    0.,
                                    self.dimensions.pixel_width as f32,
                                    tab_bar_height,
                                ),
                                metrics: &metrics,
                                gl_state: self.render_state.as_ref().unwrap(),
                                zindex: 10,
                            },
                            &Element::with_line(&font, before, palette),
                        )?;
                        elem.min_width = Some(Dimension::Pixels(measured.bounds.width()));
                    }
                    elem.max_width = Some(Dimension::Pixels(max_tab_width));
                    elem.content = match elem.content {
                        ElementContent::Text(_) => unreachable!(),
                        ElementContent::Poly { .. } => unreachable!(),
                        ElementContent::Children(mut kids) => {
                            // A group closes no tab: its tabs are closed one by one.
                            if self.config.show_close_tab_button_in_tabs && !is_group {
                                kids.push(make_x_button(&font, &metrics, &colors, tab_idx, active));
                            }
                            ElementContent::Children(kids)
                        }
                    };
                    if is_group {
                        elem = look.stack(elem, &font, active, stack_layers);
                    }
                    if active {
                        elem = look.mark(elem, &font, marker_color);
                    }
                    left_eles.push(elem);
                }
                _ => left_eles.push(item_to_elem(item)),
            }
        }

        let mut children = vec![];

        if !left_status.is_empty() {
            children.push(
                Element::new(&font, ElementContent::Children(left_status))
                    .colors(bar_colors.clone()),
            );
        }

        let window_buttons_at_left = self
            .config
            .window_decorations
            .contains(window::WindowDecorations::INTEGRATED_BUTTONS)
            && (self.config.integrated_title_button_alignment
                == IntegratedTitleButtonAlignment::Left
                || self.config.integrated_title_button_style
                    == IntegratedTitleButtonStyle::MacOsNative);

        let left_padding = if window_buttons_at_left {
            if self.config.integrated_title_button_style == IntegratedTitleButtonStyle::MacOsNative
            {
                if !self.window_state.contains(window::WindowState::FULL_SCREEN) {
                    Dimension::Pixels(70.0)
                } else {
                    Dimension::Cells(0.5)
                }
            } else {
                Dimension::Pixels(0.0)
            }
        } else {
            Dimension::Cells(0.5)
        };

        children.push(
            Element::new(&font, ElementContent::Children(left_eles))
                .vertical_align(look.vertical_align())
                .colors(bar_colors.clone())
                .padding(BoxDimension {
                    left: left_padding,
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                })
                .zindex(1),
        );
        children.push(
            Element::new(&font, ElementContent::Children(right_eles))
                .colors(bar_colors.clone())
                .float(Float::Right),
        );

        let content = ElementContent::Children(children);

        let tabs = Element::new(&font, content)
            .display(DisplayType::Block)
            .item_type(UIItemType::TabBar(TabBarItem::None))
            .min_width(Some(Dimension::Pixels(self.dimensions.pixel_width as f32)))
            .min_height(Some(Dimension::Pixels(tab_bar_height)))
            .vertical_align(look.vertical_align())
            .colors(bar_colors.clone());

        let border = self.get_os_border();

        let mut computed = self.compute_element(
            &LayoutContext {
                height: height_context,
                width: width_context,
                bounds: euclid::rect(
                    border.left.get() as f32,
                    0.,
                    self.dimensions.pixel_width as f32 - (border.left + border.right).get() as f32,
                    tab_bar_height,
                ),
                metrics: &metrics,
                gl_state: self.render_state.as_ref().unwrap(),
                zindex: 10,
            },
            &tabs,
        )?;

        computed.translate(euclid::vec2(
            0.,
            if self.config.tab_bar_at_bottom {
                self.dimensions.pixel_height as f32
                    - (computed.bounds.height() + border.bottom.get() as f32)
            } else {
                border.top.get() as f32
            },
        ));

        // The open popup: the group's tabs, drawn by the same element code as
        // the bar's tabs, in a box of the bar's color with the style's
        // corners, next to the group's item, with a point aimed at it.
        let open_group = self.open_popup_tab().and_then(|open| {
            let group_idx = self.tab_bar.folded_group_of(open)?;
            let anchor = computed.ui_items().into_iter().find(|item| {
                matches!(item.item_type,
                    UIItemType::TabBar(TabBarItem::TabGroup { group_idx: g, .. }) if g == group_idx)
            })?;
            Some((group_idx, anchor))
        });
        let popup = match open_group {
            Some((group_idx, anchor)) => {
                let group = &self.tab_bar.folded()[group_idx];
                let active_tab = mux::Mux::get()
                    .get_window(self.mux_window_id)
                    .map(|window| window.get_active_tab_idx());
                // Each tab is led by its hint, a key that activates it, in
                // quick select's label colors; the tab it was opened for is
                // marked in those colors too.
                let hints =
                    tabgrouppopup::hints(&self.config.quick_select_alphabet, group.tabs.len());
                let (hint_fg, hint_bg) = tabgrouppopup::hint_colors(&self.config, palette);
                let marked = self.get_modal().and_then(|modal| {
                    modal
                        .downcast_ref::<tabgrouppopup::TabGroupPopup>()
                        .and_then(|popup| popup.marked)
                });
                let mut entries = vec![];
                for (k, (tab_idx, active, entry)) in
                    group.entries(active_tab).into_iter().enumerate()
                {
                    if let Some(hint) = hints.get(k) {
                        entries.push(
                            Element::new(&font, ElementContent::Text(hint.to_string()))
                                .vertical_align(VerticalAlign::Middle)
                                .margin(BoxDimension {
                                    left: Dimension::Cells(0.3),
                                    right: Dimension::Cells(0.15),
                                    top: Dimension::Pixels(0.),
                                    bottom: Dimension::Pixels(0.),
                                })
                                .padding(BoxDimension {
                                    left: Dimension::Cells(0.25),
                                    right: Dimension::Cells(0.25),
                                    top: Dimension::Pixels(0.),
                                    bottom: Dimension::Pixels(0.),
                                })
                                .colors(ElementColors {
                                    border: BorderColor::new(hint_bg),
                                    bg: hint_bg.into(),
                                    text: hint_fg.into(),
                                })
                                .item_type(UIItemType::TabGroupPopup(Some(tab_idx))),
                        );
                    }
                    let mut elem = item_to_elem(&entry);
                    if active {
                        elem = look.mark(elem, &font, marker_color);
                    } else if marked == group.ids.get(k).copied() {
                        elem = look.ring(elem, &font, hint_bg);
                    }
                    elem.item_type = Some(UIItemType::TabGroupPopup(Some(tab_idx)));
                    entries.push(elem);
                }
                let edge = colors.inactive_tab_hover().bg_color.to_linear();
                let cell_height = metrics.cell_size.height as f32;
                let popup_box = Element::new(&font, ElementContent::Children(entries))
                    .item_type(UIItemType::TabGroupPopup(None))
                    .vertical_align(VerticalAlign::Middle)
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.4),
                        right: Dimension::Cells(0.4),
                        top: Dimension::Cells(0.2),
                        bottom: Dimension::Cells(0.2),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(look.popup_corners(look.height + 0.4 * cell_height + 2.))
                    .colors(ElementColors {
                        border: BorderColor::new(edge),
                        bg: bar_colors.bg.clone(),
                        text: bar_colors.text.clone(),
                    });
                let window_width = self.dimensions.pixel_width as f32;
                let popup_context = LayoutContext {
                    height: height_context,
                    width: width_context,
                    bounds: euclid::rect(
                        0.,
                        0.,
                        (window_width - 8.).max(1.),
                        self.dimensions.pixel_height as f32,
                    ),
                    metrics: &metrics,
                    gl_state: self.render_state.as_ref().unwrap(),
                    zindex: 100,
                };
                let mut boxed = self.compute_element(&popup_context, &popup_box)?;
                let item = euclid::rect(
                    anchor.x as f32,
                    anchor.y as f32,
                    anchor.width as f32,
                    anchor.height as f32,
                );
                let point_height = (cell_height * 0.5).round().max(4.);
                let point_width = (point_height * 1.6).round();
                let at_bottom = self.config.tab_bar_at_bottom;
                let origin = tabgrouppopup::popup_origin(
                    item,
                    boxed.bounds.size.to_untyped(),
                    window_width,
                    at_bottom,
                    point_height,
                    4.,
                );
                boxed.translate(euclid::vec2(origin.x, origin.y));
                let point = Element::new(
                    &font,
                    ElementContent::Poly {
                        line_width: metrics.underline_height.max(1),
                        poly: SizedPoly {
                            poly: if at_bottom {
                                TAB_GROUP_POINT_DOWN
                            } else {
                                TAB_GROUP_POINT_UP
                            },
                            width: Dimension::Pixels(point_width),
                            height: Dimension::Pixels(point_height),
                        },
                    },
                )
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::TRANSPARENT.into(),
                    text: edge.into(),
                });
                let mut point = self.compute_element(&popup_context, &point)?;
                let point_x = tabgrouppopup::point_x(
                    item,
                    origin.x,
                    boxed.bounds.width(),
                    point_width,
                    look.corner_x.max(cell_height / 2.),
                );
                let point_y = if at_bottom {
                    origin.y + boxed.bounds.height() - 1.
                } else {
                    origin.y - point_height + 1.
                };
                point.translate(euclid::vec2(point_x, point_y));
                Some(vec![boxed, point])
            }
            None => None,
        };

        Ok((computed, popup))
    }

    pub fn paint_fancy_tab_bar(&self) -> anyhow::Result<Vec<UIItem>> {
        let computed = self.fancy_tab_bar.as_ref().ok_or_else(|| {
            anyhow::anyhow!("paint_fancy_tab_bar called but fancy_tab_bar is None")
        })?;
        let ui_items = computed.ui_items();

        let gl_state = self.render_state.as_ref().unwrap();
        self.render_element(&computed, gl_state, None)?;

        Ok(ui_items)
    }
}

fn make_x_button(
    font: &Rc<LoadedFont>,
    metrics: &RenderMetrics,
    colors: &TabBarColors,
    tab_idx: usize,
    active: bool,
) -> Element {
    Element::new(
        &font,
        ElementContent::Poly {
            line_width: metrics.underline_height.max(2),
            poly: SizedPoly {
                poly: X_BUTTON,
                width: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
                height: Dimension::Pixels(metrics.cell_size.height as f32 / 2.),
            },
        },
    )
    // Ensure that we draw our background over the
    // top of the rest of the tab contents
    .zindex(1)
    .vertical_align(VerticalAlign::Middle)
    .float(Float::Right)
    .item_type(UIItemType::CloseTab(tab_idx))
    .hover_colors({
        let inactive_tab_hover = colors.inactive_tab_hover();
        let active_tab = colors.active_tab();

        Some(ElementColors {
            border: BorderColor::default(),
            bg: (if active {
                inactive_tab_hover.bg_color
            } else {
                active_tab.bg_color
            })
            .to_linear()
            .into(),
            text: (if active {
                inactive_tab_hover.fg_color
            } else {
                active_tab.fg_color
            })
            .to_linear()
            .into(),
        })
    })
    .padding(BoxDimension {
        left: Dimension::Cells(0.25),
        right: Dimension::Cells(0.25),
        top: Dimension::Cells(0.25),
        bottom: Dimension::Cells(0.25),
    })
    .margin(BoxDimension {
        left: Dimension::Cells(0.5),
        right: Dimension::Cells(0.),
        top: Dimension::Cells(0.),
        bottom: Dimension::Cells(0.),
    })
}

#[cfg(test)]
mod test {
    use super::*;

    const WIDTH: DimensionContext = DimensionContext {
        dpi: 144.,
        pixel_max: 3840.,
        pixel_cell: 21.,
    };
    const HEIGHT: DimensionContext = DimensionContext {
        dpi: 144.,
        pixel_max: 2160.,
        pixel_cell: 27.,
    };

    fn look(style: FancyTabBarStyle, is_bottom: bool) -> TabLook {
        TabLook::new(&style, is_bottom, WIDTH, HEIGHT)
    }

    fn pixels(b: BoxDimension) -> PixelDimension {
        PixelDimension {
            left: b.left.evaluate_as_pixels(WIDTH),
            right: b.right.evaluate_as_pixels(WIDTH),
            top: b.top.evaluate_as_pixels(HEIGHT),
            bottom: b.bottom.evaluate_as_pixels(HEIGHT),
        }
    }

    fn sides(left: f32, right: f32, top: f32, bottom: f32) -> PixelDimension {
        PixelDimension {
            left,
            right,
            top,
            bottom,
        }
    }

    /// The corners the tab arms built before the style existed.
    fn rounded_before(active: bool, is_bottom: bool) -> Corners {
        let round = |poly| SizedPoly {
            width: Dimension::Cells(0.5),
            height: Dimension::Cells(0.5),
            poly,
        };
        let near = if active {
            SizedPoly::none()
        } else {
            SizedPoly {
                width: Dimension::Cells(0.),
                height: Dimension::Cells(0.33),
                poly: &[],
            }
        };
        if is_bottom {
            Corners {
                top_left: near,
                top_right: near,
                bottom_left: round(BOTTOM_LEFT_ROUNDED_CORNER),
                bottom_right: round(BOTTOM_RIGHT_ROUNDED_CORNER),
            }
        } else {
            Corners {
                top_left: round(TOP_LEFT_ROUNDED_CORNER),
                top_right: round(TOP_RIGHT_ROUNDED_CORNER),
                bottom_left: near,
                bottom_right: near,
            }
        }
    }

    #[test]
    fn default_style_draws_the_tab_as_before() {
        for is_bottom in [true, false] {
            let look = look(FancyTabBarStyle::default(), is_bottom);
            assert_eq!(pixels(look.padding()), sides(10., 10., 6., 6.));
            let gap = (0.2f32 * 27.).floor();
            assert_eq!(
                pixels(look.margin()),
                if is_bottom {
                    sides(0., 0., 0., gap)
                } else {
                    sides(0., 0., gap, 0.)
                }
            );
            assert_eq!(
                look.vertical_align(),
                if is_bottom {
                    VerticalAlign::Top
                } else {
                    VerticalAlign::Bottom
                }
            );
            for active in [true, false] {
                assert_eq!(
                    look.corners(active, look.height),
                    Some(rounded_before(active, is_bottom))
                );
            }
            assert_eq!(look.marker_split(), None);
        }
    }

    #[test]
    fn pill_rounds_every_corner_to_half_the_height() {
        let look = look(
            FancyTabBarStyle {
                tab_shape: TabShape::Pill,
                ..Default::default()
            },
            true,
        );
        let corners = look.corners(false, 40.).unwrap();
        for corner in [
            corners.top_left,
            corners.top_right,
            corners.bottom_left,
            corners.bottom_right,
        ] {
            assert_eq!(corner.width, Dimension::Pixels(20.));
            assert_eq!(corner.height, Dimension::Pixels(20.));
        }
        assert_eq!(corners.top_left.poly, TOP_LEFT_ROUNDED_CORNER);
        assert_eq!(corners.bottom_right.poly, BOTTOM_RIGHT_ROUNDED_CORNER);
    }

    #[test]
    fn slant_cuts_opposite_corners_over_the_full_height() {
        let look = look(
            FancyTabBarStyle {
                tab_shape: TabShape::Slant,
                ..Default::default()
            },
            true,
        );
        let corners = look.corners(true, 40.).unwrap();
        let run = Dimension::Pixels(10.);
        assert_eq!(corners.top_left.poly, TOP_LEFT_SLANT_CORNER);
        assert_eq!(corners.bottom_right.poly, BOTTOM_RIGHT_SLANT_CORNER);
        for cut in [corners.top_left, corners.bottom_right] {
            assert_eq!((cut.width, cut.height), (run, Dimension::Pixels(40.)));
        }
        for uncut in [corners.top_right, corners.bottom_left] {
            assert_eq!((uncut.width, uncut.height), (run, Dimension::Pixels(0.)));
        }
    }

    #[test]
    fn square_has_no_corners() {
        let look = look(
            FancyTabBarStyle {
                tab_shape: TabShape::Square,
                ..Default::default()
            },
            true,
        );
        assert_eq!(look.corners(true, 40.), None);
    }

    #[test]
    fn a_marker_keeps_the_tab_its_size() {
        for (marker, expected_outer) in [
            (TabMarker::TopLine, sides(0., 0., 2., 0.)),
            (TabMarker::BottomLine, sides(0., 0., 0., 2.)),
            (TabMarker::Ring, sides(2., 2., 2., 2.)),
        ] {
            let look = look(
                FancyTabBarStyle {
                    active_tab_marker: marker,
                    ..Default::default()
                },
                true,
            );
            let (outer, inner) = look.marker_split().unwrap();
            assert_eq!(outer, expected_outer, "{marker:?}");
            assert_eq!(
                sides(
                    outer.left + inner.left,
                    outer.right + inner.right,
                    outer.top + inner.top,
                    outer.bottom + inner.bottom,
                ),
                pixels(look.padding()),
                "{marker:?}"
            );
        }
    }

    #[test]
    fn middle_drops_the_far_edge_gap_and_keeps_the_tab_gap() {
        let look = look(
            FancyTabBarStyle {
                tab_vertical_align: TabVerticalAlign::Middle,
                tab_gap: Dimension::Pixels(-3.),
                ..Default::default()
            },
            true,
        );
        assert_eq!(look.vertical_align(), VerticalAlign::Middle);
        assert_eq!(pixels(look.margin()), sides(0., -3., 0., 0.));
    }

    fn text(line: &Line) -> String {
        (0..line.len())
            .filter_map(|i| line.get_cell(i).map(|c| c.str().to_string()))
            .collect()
    }

    #[test]
    fn a_hint_takes_the_place_of_a_groups_numbers() {
        let seq = termwiz::surface::SEQ_ZERO;
        let plain = termwiz::cell::CellAttributes::default();
        let title = Line::from_text(" 3\u{2013}5: philemon ", &plain, seq, None);
        let fg = ColorAttribute::PaletteIndex(0);
        let bg = ColorAttribute::PaletteIndex(1);
        let hinted = hinted_title(&title, 6, 's', fg, bg);
        assert_eq!(text(&hinted), " s: philemon ");
        let hint = hinted.get_cell(1).unwrap();
        assert_eq!(
            (hint.attrs().foreground(), hint.attrs().background()),
            (fg, bg)
        );
        assert_eq!(
            hinted.get_cell(0).unwrap().attrs().background(),
            ColorAttribute::Default
        );
    }

    #[test]
    fn a_hint_with_no_numbers_to_replace_is_added_in_front_of_the_label() {
        let seq = termwiz::surface::SEQ_ZERO;
        let plain = termwiz::cell::CellAttributes::default();
        let title = Line::from_text(" philemon ", &plain, seq, None);
        let hinted = hinted_title(
            &title,
            1,
            'a',
            ColorAttribute::Default,
            ColorAttribute::Default,
        );
        assert_eq!(text(&hinted), " a: philemon ");
    }

    #[test]
    fn a_dimmed_title_loses_its_own_background_and_takes_the_dim_color() {
        let seq = termwiz::surface::SEQ_ZERO;
        let tint = ColorAttribute::PaletteIndex(2);
        let mut attrs = termwiz::cell::CellAttributes::default();
        attrs.set_background(tint);
        let title = Line::from_text(" 7: braid ", &attrs, seq, None);
        let dim = ColorAttribute::PaletteIndex(8);
        let dimmed = dimmed_title(&title, dim);
        assert_eq!(text(&dimmed), " 7: braid ");
        for i in 0..dimmed.len() {
            let cell = dimmed.get_cell(i).unwrap();
            assert_eq!(cell.attrs().foreground(), dim);
            assert_eq!(cell.attrs().background(), ColorAttribute::Default);
        }
    }

    #[test]
    fn hint_letters_follow_quick_select_alphabet() {
        assert_eq!(
            tabgrouppopup::hints("ASdf", 3)
                .into_iter()
                .collect::<String>(),
            "asd"
        );
    }

    #[test]
    fn a_title_background_paints_where_the_style_says() {
        let tint = ColorAttribute::TrueColorWithDefaultFallback(termwiz::color::SrgbaTuple(
            0.1, 0.2, 0.3, 1.0,
        ));
        let mut attrs = termwiz::cell::CellAttributes::default();
        attrs.set_background(tint);
        let title = Line::from_text(" 1: x ", &attrs, termwiz::surface::SEQ_ZERO, None);
        let colors = |line: &Line| {
            let cell = line.get_cell(1).unwrap();
            (cell.attrs().foreground(), cell.attrs().background())
        };

        let fill = paint_title(&title, TitleBackgroundPaint::Fill, true);
        assert!(matches!(fill, Cow::Borrowed(_)));
        assert_eq!(colors(&fill), (ColorAttribute::Default, tint));

        let text = paint_title(&title, TitleBackgroundPaint::Text, true);
        assert_eq!(colors(&text), (tint, ColorAttribute::Default));

        for paint in [TitleBackgroundPaint::Fill, TitleBackgroundPaint::Text] {
            let hidden = paint_title(&title, paint, false);
            assert_eq!(
                colors(&hidden),
                (ColorAttribute::Default, ColorAttribute::Default),
                "{paint:?}"
            );
        }

        let plain = Line::from_text(
            " 2: y ",
            &termwiz::cell::CellAttributes::default(),
            termwiz::surface::SEQ_ZERO,
            None,
        );
        assert!(matches!(
            paint_title(&plain, TitleBackgroundPaint::Text, true),
            Cow::Borrowed(_)
        ));
    }
}
