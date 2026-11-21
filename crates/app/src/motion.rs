//! Política de movimento do desktop. O estado interpolado nunca é persistido.

use gpui_kit::base::{self, Interpolate};
use gpui_kit::component::{Selectable, menu::PopupMenu, popover::Popover};
use gpui_kit::*;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const FAST: Duration = Duration::from_millis(120);
pub const BASE: Duration = Duration::from_millis(180);
pub const SLOW: Duration = Duration::from_millis(260);

pub fn disabled() -> bool {
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| std::env::var("DBM_NO_ANIM").is_ok_and(|v| v == "1"))
}

pub fn easing() -> base::Easing {
    base::Easing::cubic_bezier(0.2, 0.8, 0.2, 1.).expect("curva de movimento válida")
}

pub fn ease(progress: f32) -> f32 {
    easing().sample(progress.clamp(0., 1.))
}

/// Mistura RGB para não percorrer outros matizes entre o cinza e o acento.
pub fn color(from: Hsla, to: Hsla, progress: f32) -> Hsla {
    let (from, to) = (Rgba::from(from), Rgba::from(to));
    let mix = |a, b| a + (b - a) * progress;
    Rgba {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
    .into()
}

pub fn animation(duration: Duration) -> Animation {
    Animation::new(if disabled() { Duration::ZERO } else { duration }).with_easing(ease)
}

pub fn value<T: Interpolate + PartialEq + 'static>(
    id: impl Into<ElementId>,
    target: T,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> T {
    base::transition(
        id.into(),
        target,
        base::Transition::new(if disabled() { Duration::ZERO } else { duration }).easing(easing()),
        window,
        cx,
    )
}

/// Tween do canvas; uma nova ação parte do valor visível, sem saltar ao destino anterior.
pub struct Tween<const N: usize> {
    pub to: [f32; N],
    from: [f32; N],
    started: Instant,
    duration: Duration,
}

impl<const N: usize> Tween<N> {
    pub fn new(from: [f32; N], to: [f32; N], duration: Duration) -> Self {
        Self {
            from,
            to,
            started: Instant::now(),
            duration,
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.from[0] += dx;
        self.to[0] += dx;
        self.from[1] += dy;
        self.to[1] += dy;
    }

    pub fn sample(&self, now: Instant) -> [f32; N] {
        if self.finished(now) {
            return self.to;
        }
        let progress = ease(
            now.saturating_duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32(),
        );
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * progress)
    }

    pub fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.duration
    }
}

/// Transição de um controle sem uma entidade de aplicação por botão.
#[derive(IntoElement)]
pub struct Animated<E: IntoElement + 'static> {
    id: ElementId,
    target: f32,
    element: E,
    apply: Box<dyn FnOnce(E, f32) -> E>,
}

pub fn animate<E: IntoElement + 'static>(
    id: impl Into<ElementId>,
    target: f32,
    element: E,
    apply: impl FnOnce(E, f32) -> E + 'static,
) -> Animated<E> {
    Animated {
        id: id.into(),
        target,
        element,
        apply: Box::new(apply),
    }
}

impl<E: IntoElement + 'static> RenderOnce for Animated<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let progress = value(self.id, self.target, BASE, window, cx);
        (self.apply)(self.element, progress)
    }
}

#[derive(IntoElement)]
pub struct Reveal {
    id: ElementId,
    open: bool,
    content: AnyElement,
}

pub fn reveal(id: impl Into<ElementId>, open: bool, content: impl IntoElement) -> Reveal {
    Reveal {
        id: id.into(),
        open,
        content: content.into_any_element(),
    }
}

impl RenderOnce for Reveal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let progress = value(
            (self.id.clone(), "progress"),
            f32::from(self.open),
            BASE,
            window,
            cx,
        );
        reveal_at(self.id, progress, self.content, cx)
    }
}

pub fn reveal_at(
    id: impl Into<ElementId>,
    progress: f32,
    content: impl IntoElement,
    cx: &App,
) -> AnyElement {
    if progress == 0. {
        div().into_any_element()
    } else if disabled() || cx.reduce_motion() {
        content.into_any_element()
    } else {
        base::MotionReveal::new(id, progress, content.into_any_element()).into_any_element()
    }
}

pub fn enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    element: E,
) -> impl IntoElement {
    element.with_animation(id, animation(BASE), |el, t| {
        el.opacity(t).top(px(-6. * (1. - t)))
    })
}

/// O menu mantém navegação, atalhos e dismiss do kit; só a superfície recebe movimento.
// shortcut: submenus internos são instantâneos; animar quando o kit expuser sua superfície.
pub fn dropdown(
    id: impl Into<ElementId>,
    trigger: impl Selectable + IntoElement + 'static,
    anchor: Anchor,
    focus: FocusHandle,
    build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
) -> Popover {
    let id = id.into();
    Popover::new(id.clone())
        .appearance(false)
        .overlay_closable(false)
        .anchor(anchor)
        .trigger(trigger)
        .content(move |_, window, cx| {
            let state =
                window.use_keyed_state((id.clone(), "menu"), cx, |_, _| None::<Entity<PopupMenu>>);
            let existing = state.read(cx).clone();
            let menu = existing.unwrap_or_else(|| {
                let menu = PopupMenu::build(window, cx, |menu, window, cx| {
                    build(menu.action_context(focus.clone()), window, cx)
                });
                state.update(cx, |state, _| *state = Some(menu.clone()));
                menu.focus_handle(cx).focus(window, cx);
                let popover = cx.entity().downgrade();
                let state = state.downgrade();
                window
                    .subscribe(&menu, cx, move |_, _: &DismissEvent, window, cx| {
                        _ = popover.update(cx, |p, cx| p.dismiss(window, cx));
                        _ = state.update(cx, |s, _| *s = None);
                    })
                    .detach();
                menu
            });
            enter(
                ("menu-enter", menu.entity_id()),
                div().relative().child(menu),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{BASE, SLOW, Tween, color, ease};
    use gpui_kit::{Rgba, rgb};
    use std::time::{Duration, Instant};

    #[test]
    fn curve_matches_css_and_never_overshoots() {
        assert_eq!(ease(0.), 0.);
        assert_eq!(ease(1.), 1.);
        // x(0,5) = 0,275 e y(0,5) = 0,8 para cubic-bezier(.2,.8,.2,1).
        assert!((ease(0.275) - 0.8).abs() < 0.0001);
        for step in 1..=100 {
            let t = step as f32 / 100.;
            assert!(ease(t) >= ease(t - 0.01));
            assert!((0. ..=1.).contains(&ease(t)));
        }
    }

    #[test]
    fn color_fade_does_not_introduce_an_intermediate_hue() {
        let middle = Rgba::from(color(rgb(0xff0000).into(), rgb(0x00ffff).into(), 0.5));
        for channel in [middle.r, middle.g, middle.b] {
            assert!((channel - 0.5).abs() < 0.0001);
        }
    }

    #[test]
    fn tween_finishes_exactly_and_can_reverse_from_visible_position() {
        let first = Tween::new([0., 100., 1.], [100., -100., 2.], SLOW);
        assert_eq!(first.sample(first.started), [0., 100., 1.]);
        let now = first.started + SLOW / 2;
        let visible = first.sample(now);
        assert!(visible[0] > 50. && visible[0] < 100.);
        let second = Tween {
            from: visible,
            to: first.from,
            started: now,
            duration: BASE,
        };
        assert_eq!(second.sample(now), visible);
        assert_eq!(second.sample(now + BASE), first.from);
        assert!(second.finished(now + BASE));
        let instant = Tween::new([0.], [1.], Duration::ZERO);
        assert_eq!(instant.sample(Instant::now()), [1.]);
        let large = Tween::new([1e8], [1.], BASE);
        assert_eq!(large.sample(large.started + BASE), [1.]);
    }
}
