//! Stack spike: proves a GPUI Kit window builds and runs on this machine, with
//! the shape of the Chariot board (members and one hold you answer with a key).

use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq)]
enum HoldState {
    Open,
    Allowed,
    Denied,
}

struct Member {
    name: &'static str,
    detail: &'static str,
    level: &'static str,
    live: bool,
}

const MEMBERS: &[Member] = &[
    Member {
        name: "Claude Code",
        detail: "Claude Max · 61% quota left",
        level: "L2",
        live: true,
    },
    Member {
        name: "OpenCode",
        detail: "GLM Coding Plan",
        level: "L2",
        live: true,
    },
    Member {
        name: "Ollama",
        detail: "local models",
        level: "L1",
        live: true,
    },
    Member {
        name: "z.ai chat",
        detail: "website · assisted",
        level: "L0",
        live: false,
    },
];

struct Board {
    hold: HoldState,
}

impl Board {
    fn member_row(m: &Member, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(m.name),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(m.detail),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if m.live { "live" } else { "sandbox" }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .px_1p5()
                            .rounded_sm()
                            .border_1()
                            .border_color(theme.border)
                            .child(m.level),
                    ),
            )
    }

    fn hold_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let body = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Hold 16 · OpenCode wants to rewrite calc.js lines 10-40"),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Claude Code wrote those lines 8 minutes ago for task 9 and has not replied.",
            ));
        let footer = match self.hold {
            HoldState::Open => h_flex()
                .gap_2()
                .child(
                    Button::new("allow")
                        .primary()
                        .label("Allow")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hold = HoldState::Allowed;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("deny")
                        .outline()
                        .label("Deny")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.hold = HoldState::Denied;
                            cx.notify();
                        })),
                ),
            HoldState::Allowed => {
                h_flex().child(div().text_xs().child("Allowed. OpenCode continues."))
            }
            HoldState::Denied => h_flex().child(
                div()
                    .text_xs()
                    .child("Denied. OpenCode did the rest and left a note."),
            ),
        };
        v_flex()
            .gap_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(body)
            .child(footer)
    }
}

impl Render for Board {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted) = (theme.background, theme.foreground, theme.muted_foreground);
        let mut members = v_flex().gap_2();
        for m in MEMBERS {
            members = members.child(Self::member_row(m, cx));
        }
        v_flex()
            .size_full()
            .p_5()
            .gap_5()
            .bg(bg)
            .text_color(fg)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Chariot"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("4 members · 1 hold waiting on you"),
                    ),
            )
            .child(self.hold_card(cx))
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_xs().text_color(muted).child("MEMBERS"))
                    .child(members),
            )
    }
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        let bounds = Bounds::centered(None, size(px(520.), px(560.)), cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |_, cx| {
                cx.new(|_| Board {
                    hold: HoldState::Open,
                })
            },
        )
        .expect("failed to open window");
    });
}
