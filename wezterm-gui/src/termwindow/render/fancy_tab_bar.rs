use crate::customglyph::*;
use crate::tabbar::{TabBarItem, TabEntry};
use crate::termwindow::box_model::*;
use crate::termwindow::render::corners::*;

use crate::termwindow::render::window_buttons::window_button_element;
use crate::termwindow::{UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use config::{Dimension, DimensionContext, TabBarColors};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use termwiz::cell::CellAttributes;
use termwiz::surface::SEQ_ZERO;
use wezterm_font::LoadedFont;
use wezterm_term::color::{ColorAttribute, ColorPalette};
use wezterm_term::Line;
use window::{IntegratedTitleButtonAlignment, IntegratedTitleButtonStyle};

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

/// How many rows a fit report looks at when it counts the rows the tabs need.
const FIT_ROWS_LIMIT: usize = 8;

/// How the fancy tab bar fitted its tabs into its rows, when `tab_min_width`
/// is set. Lua reads it with `window:tab_bar_fit()`.
#[derive(Clone, Debug, PartialEq)]
pub struct TabBarFit {
    /// The rows the tab bar has now.
    pub rows: usize,
    /// The fewest rows that hold every tab with long titles cut to
    /// `tab_min_width`; None when that takes more than FIT_ROWS_LIMIT.
    pub rows_min: Option<usize>,
    /// The fewest rows that hold every tab with every title in full; None
    /// when that takes more than FIT_ROWS_LIMIT.
    pub rows_full: Option<usize>,
    /// The cells a title is cut to now; a shorter title is shown in full.
    pub title_width: usize,
    /// How many tabs each row holds now, top row first.
    pub tabs_per_row: Vec<usize>,
}

/// The tab with its title cut to `cells` cells, the last two being an
/// ellipsis and a space. A title that already fits is returned unchanged.
fn cut_title(item: &TabEntry, cells: usize) -> TabEntry {
    let mut item = item.clone();
    if item.title.len() > cells {
        let mut keep = cells.saturating_sub(2).max(1);
        // Do not split a double-width character from the cell after it.
        if keep > 1
            && item
                .title
                .get_cell(keep - 1)
                .map_or(false, |cell| cell.width() > 1)
        {
            keep -= 1;
        }
        let attrs = item
            .title
            .get_cell(keep - 1)
            .map(|cell| cell.attrs().clone())
            .unwrap_or_else(CellAttributes::default);
        item.title.resize(keep, SEQ_ZERO);
        item.title
            .append_line(Line::from_text("\u{2026} ", &attrs, SEQ_ZERO, None), SEQ_ZERO);
    }
    item
}

/// Puts the widths into rows in order, each row taking items while they fit
/// its capacity in `caps`, and a row always taking at least one. Returns each
/// row's range and whether every item was placed within `rows` rows; when
/// not, the last row carries the rest.
fn fill_rows(widths: &[f32], caps: &[f32], rows: usize) -> (Vec<Range<usize>>, bool) {
    let mut ranges = Vec::with_capacity(rows);
    let mut next = 0;
    for cap in caps.iter().take(rows) {
        let start = next;
        let mut used = 0.;
        while next < widths.len() && (next == start || used + widths[next] <= *cap) {
            used += widths[next];
            next += 1;
        }
        ranges.push(start..next);
    }
    let fits = next == widths.len();
    if !fits {
        if let Some(last) = ranges.last_mut() {
            last.end = widths.len();
        }
    }
    (ranges, fits)
}

/// The widest cut, in cells and never narrower than `floor`, at which every
/// tab fits in `rows` rows; with it, the fewest rows that hold every tab cut
/// to `floor` and in full. `widths_at(cells)` gives each tab's width with its
/// title cut to that many cells, and `max_len` is the longest title.
fn fit_titles(
    mut widths_at: impl FnMut(usize) -> anyhow::Result<Vec<f32>>,
    caps: &[f32],
    rows: usize,
    floor: usize,
    max_len: usize,
) -> anyhow::Result<(usize, Option<usize>, Option<usize>)> {
    let fits = |widths: &[f32], rows: usize| fill_rows(widths, caps, rows).1;
    let full = widths_at(max_len)?;
    let at_floor = widths_at(floor)?;
    let rows_full = (1..=FIT_ROWS_LIMIT).find(|&n| fits(&full, n));
    let rows_min = (1..=FIT_ROWS_LIMIT).find(|&n| fits(&at_floor, n));
    let title_width = if fits(&full, rows) {
        max_len
    } else if !fits(&at_floor, rows) {
        floor
    } else {
        // Every tab fits at the floor and not in full: close in on the widest
        // cut that still fits. Wider titles never need fewer rows.
        let (mut fits_at, mut too_wide) = (floor, max_len);
        while too_wide - fits_at > 1 {
            let mid = (fits_at + too_wide) / 2;
            if fits(&widths_at(mid)?, rows) {
                fits_at = mid;
            } else {
                too_wide = mid;
            }
        }
        fits_at
    };
    Ok((title_width, rows_min, rows_full))
}

impl crate::TermWindow {
    pub fn invalidate_fancy_tab_bar(&mut self) {
        self.fancy_tab_bar.take();
    }

    pub fn build_fancy_tab_bar(
        &self,
        palette: &ColorPalette,
    ) -> anyhow::Result<(ComputedElement, Option<TabBarFit>)> {
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

        let rows = self.config.tab_bar_rows.max(1);
        // One left and one right status bucket per row; only the first is
        // reachable when the bar is a single row.
        let mut row_left_status: Vec<Vec<Element>> = vec![vec![]; rows];
        let mut row_right_status: Vec<Vec<Element>> = vec![vec![]; rows];
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
        let tab_vertical_alignment = if self.config.tab_bar_at_bottom {
            VerticalAlign::Top
        } else {
            VerticalAlign::Bottom
        };

        let item_to_elem = |item: &TabEntry| -> Element {
            let element = Element::with_line(&font, &item.title, palette);

            let bg_color = item
                .title
                .get_cell(0)
                .and_then(|c| match c.attrs().background() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_bg(col)),
                });
            let fg_color = item
                .title
                .get_cell(0)
                .and_then(|c| match c.attrs().foreground() {
                    ColorAttribute::Default => None,
                    col => Some(palette.resolve_fg(col)),
                });

            let new_tab = colors.new_tab();
            let new_tab_hover = colors.new_tab_hover();
            let active_tab = colors.active_tab();
            let is_bottom = self.config.tab_bar_at_bottom;

            match item.item {
                TabBarItem::RightStatus(_) | TabBarItem::LeftStatus(_) | TabBarItem::None => element
                    .item_type(UIItemType::TabBar(TabBarItem::None))
                    .line_height(Some(1.75))
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
                TabBarItem::Tab { active, .. } if active => element
                    .vertical_align(tab_vertical_alignment)
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(if is_bottom {
                        BoxDimension {
                            left: Dimension::Cells(0.),
                            right: Dimension::Cells(0.),
                            top: Dimension::Cells(0.),
                            bottom: Dimension::Cells(0.2),
                        }
                    } else {
                        BoxDimension {
                            left: Dimension::Cells(0.),
                            right: Dimension::Cells(0.),
                            top: Dimension::Cells(0.2),
                            bottom: Dimension::Cells(0.),
                        }
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.5),
                        top: Dimension::Cells(0.25),
                        bottom: Dimension::Cells(0.25),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(if is_bottom {
                        Corners {
                            top_left: SizedPoly::none(),
                            top_right: SizedPoly::none(),
                            bottom_left: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: BOTTOM_LEFT_ROUNDED_CORNER,
                            },
                            bottom_right: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: BOTTOM_RIGHT_ROUNDED_CORNER,
                            },
                        }
                    } else {
                        Corners {
                            top_left: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: TOP_LEFT_ROUNDED_CORNER,
                            },
                            top_right: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: TOP_RIGHT_ROUNDED_CORNER,
                            },
                            bottom_left: SizedPoly::none(),
                            bottom_right: SizedPoly::none(),
                        }
                    }))
                    .colors(ElementColors {
                        border: BorderColor::new(
                            bg_color
                                .unwrap_or_else(|| active_tab.bg_color.into())
                                .to_linear(),
                        ),
                        bg: bg_color
                            .unwrap_or_else(|| active_tab.bg_color.into())
                            .to_linear()
                            .into(),
                        text: fg_color
                            .unwrap_or_else(|| active_tab.fg_color.into())
                            .to_linear()
                            .into(),
                    }),
                TabBarItem::Tab { .. } => element
                    .vertical_align(tab_vertical_alignment)
                    .item_type(UIItemType::TabBar(item.item.clone()))
                    .margin(if is_bottom {
                        BoxDimension {
                            left: Dimension::Cells(0.),
                            right: Dimension::Cells(0.),
                            top: Dimension::Cells(0.),
                            bottom: Dimension::Cells(0.2),
                        }
                    } else {
                        BoxDimension {
                            left: Dimension::Cells(0.),
                            right: Dimension::Cells(0.),
                            top: Dimension::Cells(0.2),
                            bottom: Dimension::Cells(0.),
                        }
                    })
                    .padding(BoxDimension {
                        left: Dimension::Cells(0.5),
                        right: Dimension::Cells(0.5),
                        top: Dimension::Cells(0.25),
                        bottom: Dimension::Cells(0.25),
                    })
                    .border(BoxDimension::new(Dimension::Pixels(1.)))
                    .border_corners(Some(if is_bottom {
                        Corners {
                            top_left: SizedPoly {
                                width: Dimension::Cells(0.),
                                height: Dimension::Cells(0.33),
                                poly: &[],
                            },
                            top_right: SizedPoly {
                                width: Dimension::Cells(0.),
                                height: Dimension::Cells(0.33),
                                poly: &[],
                            },
                            bottom_left: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: BOTTOM_LEFT_ROUNDED_CORNER,
                            },
                            bottom_right: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: BOTTOM_RIGHT_ROUNDED_CORNER,
                            },
                        }
                    } else {
                        Corners {
                            top_left: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: TOP_LEFT_ROUNDED_CORNER,
                            },
                            top_right: SizedPoly {
                                width: Dimension::Cells(0.5),
                                height: Dimension::Cells(0.5),
                                poly: TOP_RIGHT_ROUNDED_CORNER,
                            },
                            bottom_left: SizedPoly {
                                width: Dimension::Cells(0.),
                                height: Dimension::Cells(0.33),
                                poly: &[],
                            },
                            bottom_right: SizedPoly {
                                width: Dimension::Cells(0.),
                                height: Dimension::Cells(0.33),
                                poly: &[],
                            },
                        }
                    }))
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
                TabBarItem::NewTabButton | TabBarItem::Tab { .. } => 1.,
                _ => 0.,
            })
            .sum();
        // Each row gets its own share of the window width, so a tab may be
        // that many times wider than it would be on a single row.
        let tabs_per_row = (num_tabs / rows as f32).ceil().max(1.);
        let max_tab_width = ((self.dimensions.pixel_width as f32 / tabs_per_row)
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
            row_left_status[0].push(
                Element::new(&font, ElementContent::Text("".to_string())).margin(BoxDimension {
                    left: Dimension::Cells(4.0), // FIXME: determine exact width of macos ... buttons
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                }),
            );
        }

        let tab_elem = |item: &TabEntry, tab_idx: usize, active: bool, max_width: f32| {
            let mut elem = item_to_elem(item);
            elem.max_width = Some(Dimension::Pixels(max_width));
            elem.content = match elem.content {
                ElementContent::Text(_) => unreachable!(),
                ElementContent::Poly { .. } => unreachable!(),
                ElementContent::Children(mut kids) => {
                    if self.config.show_close_tab_button_in_tabs {
                        kids.push(make_x_button(&font, &metrics, &colors, tab_idx, active));
                    }
                    ElementContent::Children(kids)
                }
            };
            elem
        };

        // Window buttons drawn at the left come before the tabs, and the new
        // tab button after them. The right status of each row is kept as its
        // entries until the layout knows how much room it has.
        let mut lead_eles = vec![];
        let mut tab_items = vec![];
        let mut new_tab_ele = None;
        let mut row_right_entries: Vec<Vec<&TabEntry>> = vec![vec![]; rows];
        for item in items {
            match item.item {
                TabBarItem::LeftStatus(row) => {
                    if let Some(bucket) = row_left_status.get_mut(row) {
                        bucket.push(item_to_elem(item));
                    }
                }
                TabBarItem::RightStatus(row) => {
                    if let Some(bucket) = row_right_entries.get_mut(row) {
                        bucket.push(item);
                    }
                }
                TabBarItem::None => row_right_status[0].push(item_to_elem(item)),
                TabBarItem::WindowButton(_) => {
                    if self.config.integrated_title_button_alignment
                        == IntegratedTitleButtonAlignment::Left
                    {
                        lead_eles.push(item_to_elem(item))
                    } else {
                        row_right_status[0].push(item_to_elem(item))
                    }
                }
                TabBarItem::Tab { tab_idx, active } => tab_items.push((item, tab_idx, active)),
                TabBarItem::NewTabButton => new_tab_ele = Some(item_to_elem(item)),
            }
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

        let tab_run = |eles: Vec<Element>, padding_left: Dimension| {
            Element::new(&font, ElementContent::Children(eles))
                .vertical_align(tab_vertical_alignment)
                .colors(bar_colors.clone())
                .padding(BoxDimension {
                    left: padding_left,
                    right: Dimension::Cells(0.),
                    top: Dimension::Cells(0.),
                    bottom: Dimension::Cells(0.),
                })
                .zindex(1)
        };

        let border = self.get_os_border();
        let layout_context = LayoutContext {
            height: DimensionContext {
                dpi: self.dimensions.dpi as f32,
                pixel_max: self.dimensions.pixel_height as f32,
                pixel_cell: metrics.cell_size.height as f32,
            },
            width: DimensionContext {
                dpi: self.dimensions.dpi as f32,
                pixel_max: self.dimensions.pixel_width as f32,
                pixel_cell: metrics.cell_size.width as f32,
            },
            bounds: euclid::rect(
                border.left.get() as f32,
                0.,
                self.dimensions.pixel_width as f32 - (border.left + border.right).get() as f32,
                tab_bar_height,
            ),
            metrics: &metrics,
            gl_state: self.render_state.as_ref().unwrap(),
            zindex: 10,
        };

        let measure = |ele: Element| -> anyhow::Result<f32> {
            Ok(self
                .compute_element(&layout_context, &ele)?
                .bounds
                .width())
        };
        let status_width = |status: &Vec<Element>| -> anyhow::Result<f32> {
            if status.is_empty() {
                Ok(0.)
            } else {
                measure(Element::new(&font, ElementContent::Children(status.clone())))
            }
        };
        // The status of one row as elements: its entries, then any window
        // buttons drawn at the right. With a status box, a run too wide for
        // the box loses cells from its start, so its end stays visible.
        // The box is tab_bar_status_width characters wide, a character being
        // the average width of the lowercase letters and digits in the tab bar
        // font, which is what a branch name or a count is spelled in. The
        // font's cell is no guide: for a proportional font it is wider than
        // most letters, so a box sized in cells comes out much wider than the
        // text it holds.
        let status_box = if self.config.tab_bar_status_width > 0. {
            const SAMPLE: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
            let sample = measure(Element::new(
                &font,
                ElementContent::Text(SAMPLE.to_string()),
            ))?;
            self.config.tab_bar_status_width * sample / SAMPLE.len() as f32
        } else {
            0.
        };
        let right_status_eles =
            |entries: &[&TabEntry], others: Vec<Element>| -> anyhow::Result<Vec<Element>> {
                let mut eles = vec![];
                if let Some((first, rest)) = entries.split_first() {
                    let mut run = (*first).clone();
                    for entry in rest {
                        run.title.append_line(entry.title.clone(), SEQ_ZERO);
                    }
                    if status_box > 0. {
                        let width_without = |drop: usize| -> anyhow::Result<f32> {
                            let mut trimmed = run.clone();
                            for _ in 0..drop {
                                trimmed.title.remove_cell(0, SEQ_ZERO);
                            }
                            measure(item_to_elem(&trimmed))
                        };
                        if width_without(0)? > status_box {
                            // The fewest leading cells whose removal lets the run fit.
                            let (mut fits, mut too_wide) = (run.title.len(), 0);
                            while fits - too_wide > 1 {
                                let mid = (fits + too_wide) / 2;
                                if width_without(mid)? <= status_box {
                                    fits = mid;
                                } else {
                                    too_wide = mid;
                                }
                            }
                            for _ in 0..fits {
                                run.title.remove_cell(0, SEQ_ZERO);
                            }
                        }
                    }
                    eles.push(item_to_elem(&run));
                }
                eles.extend(others);
                Ok(eles)
            };

        let fit_titles_enabled = self.config.tab_min_width > 0;
        let mut fit_report = None;

        let content = if rows > 1 || fit_titles_enabled || status_box > 0. {
            // Fill the rows in order: each row takes tabs while their laid-out
            // widths fit beside that row's status areas, and the last row takes
            // whatever is left. A Block child starts a new line in the box
            // model, so one Block per row is what stacks them. Each row carries
            // its own status areas, so a second row can show status of its own
            // beside its tabs. max_tab_width is sized so that `rows` rows hold
            // every tab, so filling never needs more rows than an even split.
            //
            // With tab_bar_status_width set, the right status of every row is
            // given that fixed width instead of the width of its current text,
            // so what the status says never moves a tab to another row.
            let row_padding = |row: usize| {
                if row == 0 {
                    left_padding
                } else {
                    Dimension::Cells(0.5)
                }
            };
            // The room for tabs on each row, for as many rows as a fit report
            // may ask about; rows past the configured count have no status of
            // their own beyond the status box.
            let mut caps = vec![];
            for row in 0..rows.max(FIT_ROWS_LIMIT) {
                let left = match row_left_status.get(row) {
                    Some(status) => status_width(status)?,
                    None => 0.,
                };
                let right = if status_box > 0. {
                    status_box
                } else if row < rows {
                    status_width(&right_status_eles(
                        &row_right_entries[row],
                        row_right_status[row].clone(),
                    )?)?
                } else {
                    0.
                };
                caps.push(
                    layout_context.bounds.width()
                        - row_padding(row).evaluate_as_pixels(layout_context.width)
                        - left
                        - right,
                );
            }

            // Each row's elements, top row first.
            let mut row_tabs: Vec<Vec<Element>> = vec![];
            if fit_titles_enabled {
                // Titles are cut, never below tab_min_width, only as far as
                // the rows need; the new tab button never takes a row of its
                // own and is left out when the last row has no room for it.
                let lead_width: f32 = lead_eles
                    .iter()
                    .map(|ele| measure(ele.clone()))
                    .sum::<anyhow::Result<f32>>()?;
                caps[0] -= lead_width;
                let title_len: Vec<usize> =
                    tab_items.iter().map(|(item, _, _)| item.title.len()).collect();
                let max_len = title_len.iter().copied().max().unwrap_or(0);
                let floor = self.config.tab_min_width.min(max_len);
                let mut measured: HashMap<(usize, usize), f32> = HashMap::new();
                let mut widths_at = |cells: usize| -> anyhow::Result<Vec<f32>> {
                    let mut widths = Vec::with_capacity(tab_items.len());
                    for (i, (item, tab_idx, active)) in tab_items.iter().enumerate() {
                        let cells = cells.min(title_len[i]);
                        let width = match measured.get(&(i, cells)) {
                            Some(width) => *width,
                            None => {
                                let ele =
                                    tab_elem(&cut_title(item, cells), *tab_idx, *active, f32::MAX);
                                let width = measure(ele)?;
                                measured.insert((i, cells), width);
                                width
                            }
                        };
                        widths.push(width);
                    }
                    Ok(widths)
                };
                let (title_width, rows_min, rows_full) =
                    fit_titles(&mut widths_at, &caps, rows, floor, max_len)?;
                let widths = widths_at(title_width)?;
                let (ranges, _) = fill_rows(&widths, &caps, rows);

                let new_tab_row = match &new_tab_ele {
                    Some(ele) => {
                        let row = ranges.iter().rposition(|r| !r.is_empty()).unwrap_or(0);
                        let used: f32 = widths[ranges[row].clone()].iter().sum();
                        if used + measure(ele.clone())? <= caps[row] {
                            Some(row)
                        } else {
                            None
                        }
                    }
                    None => None,
                };
                for (row, range) in ranges.iter().enumerate() {
                    let mut mine = vec![];
                    if row == 0 {
                        mine.append(&mut lead_eles);
                    }
                    for i in range.clone() {
                        let (item, tab_idx, active) = tab_items[i];
                        let item = cut_title(item, title_width);
                        mine.push(tab_elem(&item, tab_idx, active, caps[row].max(0.)));
                    }
                    if new_tab_row == Some(row) {
                        if let Some(ele) = new_tab_ele.take() {
                            mine.push(ele);
                        }
                    }
                    row_tabs.push(mine);
                }
                fit_report = Some(TabBarFit {
                    rows,
                    rows_min,
                    rows_full,
                    title_width,
                    tabs_per_row: ranges.iter().map(|r| r.len()).collect(),
                });
            } else {
                // Each tab is capped at max_tab_width, which is sized so that
                // `rows` rows hold every tab; the last row takes whatever is left.
                let mut eles = lead_eles;
                for (item, tab_idx, active) in &tab_items {
                    eles.push(tab_elem(item, *tab_idx, *active, max_tab_width));
                }
                eles.extend(new_tab_ele.take());
                let widths = eles
                    .iter()
                    .map(|ele| measure(ele.clone()))
                    .collect::<anyhow::Result<Vec<f32>>>()?;
                let (ranges, _) = fill_rows(&widths, &caps, rows);
                let mut eles = eles.into_iter();
                for range in ranges {
                    row_tabs.push(eles.by_ref().take(range.len()).collect());
                }
            }

            let row_height = tab_bar_height / rows as f32;
            let mut row_eles = vec![];

            for (row, mine) in row_tabs.into_iter().enumerate() {
                let mut row_children = vec![];
                let left = std::mem::take(&mut row_left_status[row]);
                if !left.is_empty() {
                    row_children.push(
                        Element::new(&font, ElementContent::Children(left))
                            .colors(bar_colors.clone()),
                    );
                }
                row_children.push(tab_run(mine, row_padding(row)));
                let right = right_status_eles(
                    &row_right_entries[row],
                    std::mem::take(&mut row_right_status[row]),
                )?;
                if !right.is_empty() {
                    let mut status = Element::new(&font, ElementContent::Children(right))
                        .colors(bar_colors.clone())
                        .float(Float::Right);
                    if status_box > 0. {
                        // The box keeps its width whatever it holds, and its
                        // text sits against its right edge.
                        status = Element::new(&font, ElementContent::Children(vec![status]))
                            .colors(bar_colors.clone())
                            .min_width(Some(Dimension::Pixels(status_box)))
                            .max_width(Some(Dimension::Pixels(status_box)))
                            .float(Float::Right);
                    }
                    row_children.push(status);
                }

                // No vertical_align here: aligning a row to the bottom of the
                // whole bar makes it claim the bar's full height, which pushes
                // the next row past the bottom edge where it is clipped. The
                // tabs inside each row are aligned by tab_run instead.
                row_eles.push(
                    Element::new(&font, ElementContent::Children(row_children))
                        .display(DisplayType::Block)
                        .min_width(Some(Dimension::Pixels(self.dimensions.pixel_width as f32)))
                        .min_height(Some(Dimension::Pixels(row_height)))
                        .colors(bar_colors.clone()),
                );
            }

            ElementContent::Children(row_eles)
        } else {
            let mut children = vec![];

            let left = std::mem::take(&mut row_left_status[0]);
            if !left.is_empty() {
                children.push(
                    Element::new(&font, ElementContent::Children(left))
                        .colors(bar_colors.clone()),
                );
            }

            let mut left_eles = lead_eles;
            for (item, tab_idx, active) in &tab_items {
                left_eles.push(tab_elem(item, *tab_idx, *active, max_tab_width));
            }
            left_eles.extend(new_tab_ele.take());
            children.push(tab_run(left_eles, left_padding));
            children.push(
                Element::new(
                    &font,
                    ElementContent::Children(right_status_eles(
                        &row_right_entries[0],
                        std::mem::take(&mut row_right_status[0]),
                    )?),
                )
                .colors(bar_colors.clone())
                .float(Float::Right),
            );

            ElementContent::Children(children)
        };

        let tabs = Element::new(&font, content)
            .display(DisplayType::Block)
            .item_type(UIItemType::TabBar(TabBarItem::None))
            .min_width(Some(Dimension::Pixels(self.dimensions.pixel_width as f32)))
            .min_height(Some(Dimension::Pixels(tab_bar_height)))
            .vertical_align(tab_vertical_alignment)
            .colors(bar_colors);

        let mut computed = self.compute_element(&layout_context, &tabs)?;

        computed.translate(euclid::vec2(
            0.,
            if self.config.tab_bar_at_bottom {
                self.dimensions.pixel_height as f32
                    - (computed.bounds.height() + border.bottom.get() as f32)
            } else {
                border.top.get() as f32
            },
        ));

        Ok((computed, fit_report))
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

    #[test]
    fn fill_rows_fills_each_row_before_the_next() {
        let (ranges, fits) = fill_rows(&[3., 3., 3., 3.], &[7., 7., 7.], 2);
        assert_eq!(ranges, vec![0..2, 2..4]);
        assert!(fits);
    }

    #[test]
    fn fill_rows_puts_what_does_not_fit_on_the_last_row() {
        let (ranges, fits) = fill_rows(&[3., 3., 3., 3., 3.], &[7., 7.], 2);
        assert_eq!(ranges, vec![0..2, 2..5]);
        assert!(!fits);
    }

    #[test]
    fn fill_rows_gives_every_row_at_least_one_item() {
        let (ranges, fits) = fill_rows(&[9., 1.], &[5., 5.], 2);
        assert_eq!(ranges, vec![0..1, 1..2]);
        assert!(fits);
    }

    #[test]
    fn fit_titles_cuts_only_as_far_as_the_rows_need() {
        // Each tab is as wide as its cut in cells, capped by its own length.
        let lens = [10usize, 10, 4, 4];
        let widths_at = |cells: usize| -> anyhow::Result<Vec<f32>> {
            Ok(lens.iter().map(|&len| len.min(cells) as f32).collect())
        };
        let caps = vec![14.; FIT_ROWS_LIMIT];
        // In full the tabs need 3 rows (10 | 10 4 | 4); cut to the floor of 3
        // they need 1 (3 3 3 3). On 2 rows the widest cut that fits is 7
        // (7 7 | 4 4); at 8 the second 8 no longer fits beside the first.
        let fit = fit_titles(widths_at, &caps, 2, 3, 10).unwrap();
        assert_eq!(fit, (7, Some(1), Some(3)));
    }

    #[test]
    fn fit_titles_keeps_every_title_in_full_when_they_fit() {
        let widths_at = |cells: usize| -> anyhow::Result<Vec<f32>> {
            Ok([5usize, 5].iter().map(|&len| len.min(cells) as f32).collect())
        };
        let fit = fit_titles(widths_at, &vec![14.; FIT_ROWS_LIMIT], 1, 3, 5).unwrap();
        assert_eq!(fit, (5, Some(1), Some(1)));
    }
}
