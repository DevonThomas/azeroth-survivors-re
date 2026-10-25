//! The Escape menu and its options, drawn as 1.12's FrameXML draws them, from the client's own art:
//! `GameMenuFrame` (Main Menu), `SoundOptionsFrame` (Sound Options) at its authored geometry, and
//! the camera and mouse rows of `UIOptionsFrame` (Interface Options) on the same dialog frame. Every
//! control writes the reference's CVar, so the sound observers apply it live and the registry
//! saves it; [`apply_to_camera`] carries the camera rows onto [`CamRig`].

use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;

use super::hero::{CamRig, Follow};
use super::hud::HudFont;
use super::Phase;
use crate::cvars::Cvars;
use crate::glue::add_material::AddUiMaterial;
use crate::glue::backdrop::{backdrop_border, backdrop_edges, BackdropEdges};
use crate::sound::GlueSound;

/// `GameFontNormal`'s gold, `GameFontHighlight`'s white and `GameFontDisable`'s grey.
const NORMAL: Color = Color::srgb(1.0, 0.82, 0.0);
const HIGHLIGHT: Color = Color::WHITE;
const DISABLE: Color = Color::srgb(0.5, 0.5, 0.5);
/// `OptionFrameBoxTemplate`'s `SetBackdropColor(0.5, 0.5, 0.5)` / `SetBackdropBorderColor(0.4, …)`.
const BOX_FILL: Color = Color::srgb(0.5, 0.5, 0.5);
const BOX_EDGE: Color = Color::srgb(0.4, 0.4, 0.4);
/// Over the HUD (430s) and the results, under the tooltip.
const Z: i32 = 900;

/// `cameraDistanceMaxFactor` × this is the farthest the wheel zooms (the slider runs 1 to 2).
pub(super) const ZOOM_PER_FACTOR: f32 = 25.0;

/// Which frame is up while the game is paused.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Open {
    #[default]
    None,
    GameMenu,
    Sound,
    Interface,
}

/// The open panel and the CVars as they stood when it opened, for Cancel.
#[derive(Resource, Default)]
pub(super) struct Panels {
    pub(super) open: Open,
    initial: Vec<(&'static str, String)>,
    built: bool,
}

#[derive(Component)]
pub(super) struct Root(Open);

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Act {
    Sound,
    Interface,
    Logout,
    Exit,
    Return,
    Okay,
    Cancel,
    Defaults,
}

/// A `UIPanelButtonTemplate`: its Normal and Pushed textures, swapped on the press.
#[derive(Component)]
pub(super) struct PanelButton {
    up: Handle<Image>,
    down: Option<Handle<Image>>,
}

/// An `alphaMode="ADD"` HighlightTexture, shown while its parent is under the cursor.
#[derive(Component)]
pub(super) struct Hi;

/// An `OptionsCheckButtonTemplate` on `cvar`. `needs` greys it out while that box is clear, as
/// `SoundOptionsFrame_UpdateDependencies` does Ambience and Error Speech under Enable All Sound.
#[derive(Component)]
pub(super) struct Check {
    cvar: &'static str,
    needs: Option<&'static str>,
    label_color: Color,
    up: Handle<Image>,
    down: Option<Handle<Image>>,
}

/// The CheckedTexture (or DisabledCheckedTexture) and the label of a [`Check`].
#[derive(Component)]
pub(super) struct CheckMark {
    on: Handle<Image>,
    dis: Option<Handle<Image>>,
}
#[derive(Component)]
pub(super) struct CheckLabel;

/// An `OptionsSliderTemplate` on `cvar`, stepping by `step` between `min` and `max`.
#[derive(Component)]
pub(super) struct Slider {
    cvar: &'static str,
    min: f32,
    max: f32,
    step: f32,
}
#[derive(Component)]
pub(super) struct Thumb;

/// Camera Following Style's `UIDropDownMenuTemplate`, its text and its list.
#[derive(Component)]
pub(super) struct DropButton;
#[derive(Component)]
pub(super) struct DropText;
#[derive(Component)]
pub(super) struct DropList;
#[derive(Component)]
pub(super) struct DropItem(&'static str);
#[derive(Component)]
pub(super) struct DropCheck(&'static str);

const SLIDER_W: f32 = 128.0;
const SLIDER_H: f32 = 17.0;
const THUMB: f32 = 32.0;

/// `SoundOptionsFrameCheckButtons`: label, CVar, default, size, position, colour, dependency.
struct CheckRow {
    label: &'static str,
    cvar: &'static str,
    default: &'static str,
    size: f32,
    at: Vec2,
    white: bool,
    needs: Option<&'static str>,
}

/// `SoundOptionsFrame.xml`'s anchors, resolved: CheckButton1 at (14, 44), each box hung under the
/// one before (+6 indent on the two dependants, 24² where the XML sizes them so).
const SOUND_CHECKS: [CheckRow; 7] = [
    CheckRow {
        label: "Enable All Sound",
        cvar: "MasterSoundEffects",
        default: "1",
        size: 32.0,
        at: Vec2::new(14.0, 44.0),
        white: true,
        needs: None,
    },
    CheckRow {
        label: "Enable Ambience",
        cvar: "EnableAmbience",
        default: "1",
        size: 24.0,
        at: Vec2::new(24.0, 74.0),
        white: false,
        needs: Some("MasterSoundEffects"),
    },
    CheckRow {
        label: "Enable Error Speech",
        cvar: "EnableErrorSpeech",
        default: "1",
        size: 24.0,
        at: Vec2::new(24.0, 96.0),
        white: false,
        needs: Some("MasterSoundEffects"),
    },
    CheckRow {
        label: "Enable Music",
        cvar: "EnableMusic",
        default: "1",
        size: 32.0,
        at: Vec2::new(14.0, 120.0),
        white: true,
        needs: None,
    },
    CheckRow {
        label: "Enable Sound at Character",
        cvar: "SoundListenerAtCharacter",
        default: "1",
        size: 32.0,
        at: Vec2::new(14.0, 146.0),
        white: true,
        needs: None,
    },
    CheckRow {
        label: "Enable Emote Sounds",
        cvar: "EmoteSounds",
        default: "1",
        size: 32.0,
        at: Vec2::new(14.0, 172.0),
        white: true,
        needs: None,
    },
    CheckRow {
        label: "Loop Music",
        cvar: "SoundZoneMusicNoDelay",
        default: "0",
        size: 32.0,
        at: Vec2::new(14.0, 198.0),
        white: true,
        needs: None,
    },
];

/// A slider row: label, CVar, default, range, step, position, and its Low/High captions.
struct SliderRow {
    label: &'static str,
    cvar: &'static str,
    default: &'static str,
    min: f32,
    max: f32,
    step: f32,
    at: Vec2,
    low: &'static str,
    high: &'static str,
}

/// `SoundOptionsFrameSliders`: Slider1 TOPRIGHT (−18, −43) of the 400-wide frame, each 18 below.
const SOUND_SLIDERS: [SliderRow; 4] = [
    SliderRow {
        label: "Master Volume",
        cvar: "MasterVolume",
        default: "1",
        min: 0.0,
        max: 1.0,
        step: 0.1,
        at: Vec2::new(254.0, 43.0),
        low: "Low",
        high: "High",
    },
    SliderRow {
        label: "Sound Volume",
        cvar: "SoundVolume",
        default: "1",
        min: 0.0,
        max: 1.0,
        step: 0.1,
        at: Vec2::new(254.0, 78.0),
        low: "Low",
        high: "High",
    },
    SliderRow {
        label: "Music Volume",
        cvar: "MusicVolume",
        default: "0.4",
        min: 0.0,
        max: 1.0,
        step: 0.1,
        at: Vec2::new(254.0, 113.0),
        low: "Low",
        high: "High",
    },
    SliderRow {
        label: "Ambience Volume",
        cvar: "AmbienceVolume",
        default: "0.6",
        min: 0.0,
        max: 1.0,
        step: 0.1,
        at: Vec2::new(254.0, 148.0),
        low: "Low",
        high: "High",
    },
];

/// `UIOptionsFrameSliders`' mouse and camera rows, positions inside their boxes.
const MOUSE_SLIDER: SliderRow = SliderRow {
    label: "Mouse Sensitivity",
    cvar: "mousespeed",
    default: "1",
    min: 0.5,
    max: 1.5,
    step: 0.05,
    at: Vec2::new(20.0, 20.0),
    low: "Low",
    high: "High",
};
const CAMERA_SLIDERS: [SliderRow; 3] = [
    SliderRow {
        label: "Auto-Follow Speed",
        cvar: "cameraYawSmoothSpeed",
        default: "180",
        min: 90.0,
        max: 270.0,
        step: 10.0,
        at: Vec2::new(20.0, 80.0),
        low: "Slow",
        high: "Fast",
    },
    SliderRow {
        label: "Mouse Look Speed",
        cvar: "cameraYawMoveSpeed",
        default: "180",
        min: 90.0,
        max: 270.0,
        step: 10.0,
        at: Vec2::new(218.0, 22.0),
        low: "Slow",
        high: "Fast",
    },
    SliderRow {
        label: "Max Camera Distance",
        cvar: "cameraDistanceMaxFactor",
        default: "1",
        min: 1.0,
        max: 2.0,
        step: 0.1,
        at: Vec2::new(218.0, 80.0),
        low: "Low",
        high: "High",
    },
];
const INVERT: CheckRow = CheckRow {
    label: "Invert Mouse",
    cvar: "mouseInvertPitch",
    default: "0",
    size: 26.0,
    at: Vec2::new(188.0, 15.0),
    white: false,
    needs: None,
};
/// Camera Following Style's three entries, `cameraSmoothStyle` values as the stock dropdown writes.
const FOLLOW_STYLES: [(&str, &str); 3] = [("Smart", "1"), ("Always", "2"), ("Never", "0")];
const FOLLOW_CVAR: &str = "cameraSmoothStyle";

fn sound_cvars() -> impl Iterator<Item = (&'static str, &'static str)> {
    SOUND_CHECKS
        .iter()
        .map(|c| (c.cvar, c.default))
        .chain(SOUND_SLIDERS.iter().map(|s| (s.cvar, s.default)))
}

fn interface_cvars() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        (MOUSE_SLIDER.cvar, MOUSE_SLIDER.default),
        (INVERT.cvar, INVERT.default),
        (FOLLOW_CVAR, "1"),
    ]
    .into_iter()
    .chain(CAMERA_SLIDERS.iter().map(|s| (s.cvar, s.default)))
}

/// The client art the frames draw with, each `None` where the archives lack it.
struct Art {
    dialog_bg: Option<Handle<Image>>,
    dialog_edges: Option<BackdropEdges>,
    header: Option<Handle<Image>>,
    button_up: Option<(Handle<Image>, Vec2)>,
    button_down: Option<Handle<Image>>,
    button_hi: Option<Handle<AddUiMaterial>>,
    check_up: Option<Handle<Image>>,
    check_down: Option<Handle<Image>>,
    check_on: Option<Handle<Image>>,
    check_dis: Option<Handle<Image>>,
    check_hi: Option<Handle<AddUiMaterial>>,
    slider_bg: Option<Handle<Image>>,
    slider_edges: Option<BackdropEdges>,
    thumb: Option<Handle<Image>>,
    tip_bg: Option<Handle<Image>>,
    tip_edges: Option<BackdropEdges>,
    label_frame: Option<(Handle<Image>, Vec2)>,
    arrow_up: Option<Handle<Image>>,
    arrow_down: Option<Handle<Image>>,
    arrow_hi: Option<Handle<AddUiMaterial>>,
    item_hi: Option<Handle<AddUiMaterial>>,
}

impl Art {
    fn load(
        assets: &mut benilla_assets::WorldAssets,
        images: &mut Assets<Image>,
        adds: &mut Assets<AddUiMaterial>,
    ) -> Self {
        let mut tex = |p: &str| {
            let h = assets.sprite_texture(p, images);
            if h.is_none() {
                warn!("survivors options: no UI art {p}");
            }
            h
        };
        let dialog_bg = tex("Interface\\DialogFrame\\UI-DialogBox-Background");
        let header = tex("Interface\\DialogFrame\\UI-DialogBox-Header");
        let button_up = tex("Interface\\Buttons\\UI-Panel-Button-Up");
        let button_down = tex("Interface\\Buttons\\UI-Panel-Button-Down");
        let button_hi = tex("Interface\\Buttons\\UI-Panel-Button-Highlight");
        let check_up = tex("Interface\\Buttons\\UI-CheckBox-Up");
        let check_down = tex("Interface\\Buttons\\UI-CheckBox-Down");
        let check_on = tex("Interface\\Buttons\\UI-CheckBox-Check");
        let check_dis = tex("Interface\\Buttons\\UI-CheckBox-Check-Disabled");
        let check_hi = tex("Interface\\Buttons\\UI-CheckBox-Highlight");
        let slider_bg = tex("Interface\\Buttons\\UI-SliderBar-Background");
        let thumb = tex("Interface\\Buttons\\UI-SliderBar-Button-Horizontal");
        let tip_bg = tex("Interface\\Tooltips\\UI-Tooltip-Background");
        let label_frame = tex("Interface\\Glues\\CharacterCreate\\CharacterCreate-LabelFrame");
        let arrow_up = tex("Interface\\ChatFrame\\UI-ChatIcon-ScrollDown-Up");
        let arrow_down = tex("Interface\\ChatFrame\\UI-ChatIcon-ScrollDown-Down");
        let arrow_hi = tex("Interface\\Buttons\\UI-Common-MouseHilight");
        let item_hi = tex("Interface\\QuestFrame\\UI-QuestTitleHighlight");
        let size = |h: &Option<Handle<Image>>, images: &Assets<Image>| {
            h.as_ref()
                .and_then(|h| images.get(h).map(|i| (h.clone(), i.size_f32())))
        };
        let mut add = |h: Option<Handle<Image>>, rect: Vec4| {
            h.map(|texture| adds.add(AddUiMaterial { texture, rect }))
        };
        let full = Vec4::new(0.0, 0.0, 1.0, 1.0);
        Self {
            dialog_edges: backdrop_edges(
                assets,
                "Interface\\DialogFrame\\UI-DialogBox-Border",
                images,
            ),
            slider_edges: backdrop_edges(assets, "Interface\\Buttons\\UI-SliderBar-Border", images),
            tip_edges: backdrop_edges(assets, "Interface\\Tooltips\\UI-Tooltip-Border", images),
            button_up: size(&button_up, images),
            label_frame: size(&label_frame, images),
            // `UIPanelButtonHighlightTexture`'s TexCoords 0–0.625 × 0–0.6875.
            button_hi: add(button_hi, Vec4::new(0.0, 0.0, 0.625, 0.6875)),
            check_hi: add(check_hi, full),
            arrow_hi: add(arrow_hi, full),
            item_hi: add(item_hi, full),
            dialog_bg,
            header,
            button_down,
            check_up,
            check_down,
            check_on,
            check_dis,
            slider_bg,
            thumb,
            tip_bg,
            arrow_up,
            arrow_down,
        }
    }
}

fn px(v: f32) -> Val {
    Val::Px(v)
}

/// An absolutely placed box, in the reference's top-left anchor space (y down).
fn at(left: f32, top: f32, w: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(left),
        top: px(top),
        width: px(w),
        height: px(h),
        ..default()
    }
}

fn fill() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0.0),
        right: px(0.0),
        top: px(0.0),
        bottom: px(0.0),
        ..default()
    }
}

/// A FontString: FRIZQT at `size`, `MasterFont`'s one-unit black drop shadow.
fn label(font: &Handle<Font>, s: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
        TextShadow {
            offset: Vec2::splat(1.0),
            color: Color::BLACK,
        },
    )
}

/// The texture's TexCoords as a pixel rect.
fn crop(size: Vec2, [l, r, t, b]: [f32; 4]) -> Rect {
    Rect::new(l * size.x, t * size.y, r * size.x, b * size.y)
}

/// A tiled background at `period` UI units a tile.
fn tiled(
    img: Handle<Image>,
    images: &Assets<Image>,
    period: f32,
    scale: f32,
    color: Color,
) -> ImageNode {
    let native = images.get(&img).map_or(period, |i| i.size_f32().x.max(1.0));
    ImageNode {
        image: img,
        color,
        image_mode: NodeImageMode::Tiled {
            tile_x: true,
            tile_y: true,
            stretch_value: period * scale / native,
        },
        ..default()
    }
}

/// A `Backdrop`: the tiled `bg` inside its insets `(left, right, top, bottom)`, the `edges` round
/// the whole rect.
#[allow(clippy::too_many_arguments)]
fn backdrop(
    b: &mut ChildSpawnerCommands,
    images: &Assets<Image>,
    scale: f32,
    bg: &Option<Handle<Image>>,
    tile: f32,
    insets: [f32; 4],
    edges: &Option<BackdropEdges>,
    edge: f32,
    fill_color: Color,
    edge_color: Color,
) {
    let [l, r, t, bt] = insets;
    match bg {
        Some(bg) => {
            b.spawn((
                tiled(bg.clone(), images, tile, scale, fill_color),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(l),
                    right: px(r),
                    top: px(t),
                    bottom: px(bt),
                    ..default()
                },
            ));
        }
        None => {
            b.spawn((fill(), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85))));
        }
    }
    if let Some(edges) = edges {
        backdrop_border(b, edges, edge, edge_color);
    }
}

/// A `DIALOG`-strata frame on `UI-DialogBox-Background`/`-Border` with its `UI-DialogBox-Header`
/// and title, centred on the screen. Returns the frame to hang controls on.
fn dialog(
    commands: &mut Commands,
    art: &Art,
    images: &Assets<Image>,
    font: &Handle<Font>,
    scale: f32,
    which: Open,
    size: Vec2,
    title: &str,
) -> Entity {
    let mut frame = Entity::PLACEHOLDER;
    commands
        .spawn((
            Root(which),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(Z),
            Visibility::Hidden,
            bevy::ui::FocusPolicy::Pass,
        ))
        .with_children(|root| {
            frame = root
                .spawn((
                    Node {
                        width: px(size.x),
                        height: px(size.y),
                        ..default()
                    },
                    // `enableMouse`: the frame swallows clicks, so none reaches the world.
                    Interaction::default(),
                ))
                .with_children(|f| {
                    backdrop(
                        f,
                        images,
                        scale,
                        &art.dialog_bg,
                        32.0,
                        [11.0, 12.0, 12.0, 11.0],
                        &art.dialog_edges,
                        32.0,
                        Color::WHITE,
                        Color::WHITE,
                    );
                    // The header, 256×64 at TOP (0, 12); the title 14 below its top.
                    if let Some(h) = &art.header {
                        f.spawn((
                            ImageNode::new(h.clone()),
                            at((size.x - 256.0) / 2.0, -12.0, 256.0, 64.0),
                        ));
                    }
                    f.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            right: px(0.0),
                            top: px(2.0),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        children![label(font, title, 12.0, NORMAL)],
                    ));
                })
                .id();
        });
    frame
}

/// A `GameMenuButtonTemplate` (`UIPanelButtonTemplate`, GameFontHighlight caption) at `node`.
fn panel_button(
    b: &mut ChildSpawnerCommands,
    art: &Art,
    font: &Handle<Font>,
    act: Act,
    caption: &str,
    node: Node,
) {
    let mut e = b.spawn((
        act,
        Button,
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..node
        },
    ));
    match &art.button_up {
        Some((up, size)) => {
            let tc = [0.0, 0.625, 0.0, 0.6875];
            e.insert((
                ImageNode {
                    image: up.clone(),
                    rect: Some(crop(*size, tc)),
                    ..default()
                },
                PanelButton {
                    up: up.clone(),
                    down: art.button_down.clone(),
                },
            ));
        }
        None => {
            e.insert(BackgroundColor(Color::srgb(0.45, 0.05, 0.02)));
        }
    }
    e.with_children(|inner| {
        if let Some(hi) = &art.button_hi {
            inner.spawn((Hi, Visibility::Hidden, MaterialNode(hi.clone()), fill()));
        }
        inner.spawn(label(font, caption, 12.0, HIGHLIGHT));
    });
}

/// An `OptionsCheckButtonTemplate` with its `$parentText` at RIGHT (−2, 0).
fn check_button(b: &mut ChildSpawnerCommands, art: &Art, font: &Handle<Font>, row: &CheckRow) {
    let Some(up) = art.check_up.clone() else {
        return;
    };
    let color = if row.white { HIGHLIGHT } else { NORMAL };
    b.spawn((
        Check {
            cvar: row.cvar,
            needs: row.needs,
            label_color: color,
            up: up.clone(),
            down: art.check_down.clone(),
        },
        Button,
        ImageNode::new(up),
        at(row.at.x, row.at.y, row.size, row.size),
    ))
    .with_children(|c| {
        if let Some(on) = &art.check_on {
            c.spawn((
                CheckMark {
                    on: on.clone(),
                    dis: art.check_dis.clone(),
                },
                ImageNode::new(on.clone()),
                Visibility::Hidden,
                fill(),
            ));
        }
        if let Some(hi) = &art.check_hi {
            c.spawn((Hi, Visibility::Hidden, MaterialNode(hi.clone()), fill()));
        }
        // The label, and `HitRectInsets` right −100: clicking the words toggles it too.
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(row.size - 2.0),
                top: px(0.0),
                height: px(row.size),
                align_items: AlignItems::Center,
                ..default()
            },
            children![(CheckLabel, label(font, row.label, 10.0, color))],
        ));
    });
}

/// An `OptionsSliderTemplate`: the bar's backdrop, the 32² thumb, the caption over it and the
/// Low/High under it.
fn slider(
    b: &mut ChildSpawnerCommands,
    art: &Art,
    images: &Assets<Image>,
    font: &Handle<Font>,
    scale: f32,
    row: &SliderRow,
) {
    let (x, y) = (row.at.x, row.at.y);
    // `$parentText` BOTTOM to the bar's TOP.
    b.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(x - 40.0),
            width: px(SLIDER_W + 80.0),
            top: px(y - 13.0),
            height: px(13.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            ..default()
        },
        children![label(font, row.label, 10.0, NORMAL)],
    ));
    b.spawn((
        Slider {
            cvar: row.cvar,
            min: row.min,
            max: row.max,
            step: row.step,
        },
        Interaction::default(),
        RelativeCursorPosition::default(),
        at(x, y, SLIDER_W, SLIDER_H),
    ))
    .with_children(|s| {
        backdrop(
            s,
            images,
            scale,
            &art.slider_bg,
            8.0,
            [3.0, 3.0, 6.0, 6.0],
            &art.slider_edges,
            8.0,
            Color::WHITE,
            Color::WHITE,
        );
        if let Some(thumb) = &art.thumb {
            s.spawn((
                Thumb,
                ImageNode::new(thumb.clone()),
                at(0.0, (SLIDER_H - THUMB) / 2.0, THUMB, THUMB),
            ));
        }
    });
    // `$parentLow` TOPLEFT to BOTTOMLEFT (2, 3), `$parentHigh` TOPRIGHT to BOTTOMRIGHT (−2, 3).
    b.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(x + 2.0),
            width: px(SLIDER_W - 4.0),
            top: px(y + SLIDER_H - 3.0),
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        children![
            label(font, row.low, 10.0, HIGHLIGHT),
            label(font, row.high, 10.0, HIGHLIGHT)
        ],
    ));
}

/// An `OptionFrameBoxTemplate`: tooltip backdrop tinted 0.5/0.4, its title above it, 9 in.
#[allow(clippy::too_many_arguments)]
fn option_box(
    b: &mut ChildSpawnerCommands,
    art: &Art,
    images: &Assets<Image>,
    font: &Handle<Font>,
    scale: f32,
    title: &str,
    node: Node,
    inner: impl FnOnce(&mut ChildSpawnerCommands),
) {
    b.spawn(node).with_children(|bx| {
        backdrop(
            bx,
            images,
            scale,
            &art.tip_bg,
            16.0,
            [5.0, 5.0, 5.0, 5.0],
            &art.tip_edges,
            16.0,
            BOX_FILL,
            BOX_EDGE,
        );
        bx.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(9.0),
                top: px(-13.0),
                ..default()
            },
            children![label(font, title, 12.0, HIGHLIGHT)],
        ));
        inner(bx);
    });
}

/// `UIDropDownMenuTemplate` at `UIDropDownMenu_SetWidth(90)`: the LabelFrame's three pieces hung
/// 17 above, the selection right-justified 43 in from the right piece, the arrow button, and the
/// list it drops (a dialog backdrop, a check by the chosen entry).
#[allow(clippy::too_many_arguments)]
fn dropdown(
    b: &mut ChildSpawnerCommands,
    art: &Art,
    images: &Assets<Image>,
    font: &Handle<Font>,
    scale: f32,
    pos: Vec2,
    caption: &str,
) {
    const MID: f32 = 90.0;
    let w = 25.0 + MID + 25.0;
    b.spawn(at(pos.x, pos.y, w, 32.0)).with_children(|d| {
        // `$parentLabel` BOTTOMLEFT to TOPLEFT (16, 3).
        d.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(16.0),
                bottom: px(32.0 + 3.0),
                ..default()
            },
            children![label(font, caption, 10.0, NORMAL)],
        ));
        if let Some((frame, size)) = &art.label_frame {
            for (left, width, tc) in [
                (0.0, 25.0, [0.0, 0.195_312_5, 0.0, 1.0]),
                (25.0, MID, [0.195_312_5, 0.804_687_5, 0.0, 1.0]),
                (25.0 + MID, 25.0, [0.804_687_5, 1.0, 0.0, 1.0]),
            ] {
                d.spawn((
                    ImageNode {
                        image: frame.clone(),
                        rect: Some(crop(*size, tc)),
                        ..default()
                    },
                    at(left, -17.0, width, 64.0),
                ));
            }
        }
        // `$parentText` RIGHT to the right piece's RIGHT (−43, 2): the right piece ends at `w`.
        d.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(43.0),
                top: px(-17.0 + 32.0 - 2.0 - 5.0),
                height: px(10.0),
                align_items: AlignItems::Center,
                ..default()
            },
            children![(DropText, label(font, "", 10.0, HIGHLIGHT))],
        ));
        // `$parentButton` 24² at the right piece's TOPRIGHT (−16, −18).
        let mut btn = d.spawn((
            DropButton,
            Button,
            at(w - 16.0 - 24.0, -17.0 + 18.0, 24.0, 24.0),
        ));
        if let Some(up) = &art.arrow_up {
            btn.insert((
                ImageNode::new(up.clone()),
                PanelButton {
                    up: up.clone(),
                    down: art.arrow_down.clone(),
                },
            ));
        }
        btn.with_children(|bt| {
            if let Some(hi) = &art.arrow_hi {
                bt.spawn((Hi, Visibility::Hidden, MaterialNode(hi.clone()), fill()));
            }
        });
        // The list, under the button's left edge, as `ToggleDropDownMenu` anchors it.
        let item_h = 16.0;
        let list_h = 15.0 * 2.0 + item_h * FOLLOW_STYLES.len() as f32;
        d.spawn((
            DropList,
            Node {
                display: Display::None,
                ..at(w - 16.0 - 24.0 - 8.0, 30.0, 110.0, list_h)
            },
            Visibility::Hidden,
            GlobalZIndex(Z + 1),
            Interaction::default(),
        ))
        .with_children(|l| {
            backdrop(
                l,
                images,
                scale,
                &art.dialog_bg,
                32.0,
                [11.0, 12.0, 12.0, 11.0],
                &art.dialog_edges,
                32.0,
                Color::WHITE,
                Color::WHITE,
            );
            for (i, (name, value)) in FOLLOW_STYLES.iter().enumerate() {
                l.spawn((
                    DropItem(value),
                    Button,
                    at(15.0, 15.0 + i as f32 * item_h, 110.0 - 30.0, item_h),
                ))
                .with_children(|it| {
                    if let Some(hi) = &art.item_hi {
                        it.spawn((Hi, Visibility::Hidden, MaterialNode(hi.clone()), fill()));
                    }
                    if let Some(on) = &art.check_on {
                        it.spawn((
                            DropCheck(value),
                            ImageNode::new(on.clone()),
                            Visibility::Hidden,
                            at(-4.0, -4.0, 24.0, 24.0),
                        ));
                    }
                    it.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(20.0),
                            top: px(0.0),
                            height: px(item_h),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        children![label(font, name, 10.0, HIGHLIGHT)],
                    ));
                });
            }
        });
    });
}

/// Build the three frames once the archives can be read.
pub(super) fn build(
    mut commands: Commands,
    mut panels: ResMut<Panels>,
    assets: Option<ResMut<benilla_assets::WorldAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut adds: ResMut<Assets<AddUiMaterial>>,
    font: Option<Res<HudFont>>,
    scale: Res<UiScale>,
) {
    if panels.built {
        return;
    }
    let (Some(mut assets), Some(font)) = (assets, font) else {
        return;
    };
    panels.built = true;
    let art = Art::load(&mut assets, &mut images, &mut adds);
    let f = &font.0;
    let s = scale.0;

    // GameMenuFrame: 144×21 buttons, the first CENTER at TOP −37, each 1 under the last, Return to
    // Game 16 further. Video Options, Key Bindings and Macros have nothing behind them here.
    let rows = [
        (Act::Sound, "Sound Options"),
        (Act::Interface, "Interface Options"),
        (Act::Logout, "Logout"),
        (Act::Exit, "Exit Game"),
    ];
    let last = 26.5 + 22.0 * rows.len() as f32 + 15.0;
    let menu = dialog(
        &mut commands,
        &art,
        &images,
        f,
        s,
        Open::GameMenu,
        Vec2::new(195.0, last + 21.0 + 29.5),
        "Main Menu",
    );
    commands.entity(menu).with_children(|m| {
        for (i, (act, caption)) in rows.iter().enumerate() {
            panel_button(
                m,
                &art,
                f,
                *act,
                caption,
                at(25.5, 26.5 + 22.0 * i as f32, 144.0, 21.0),
            );
        }
        panel_button(
            m,
            &art,
            f,
            Act::Return,
            "Return to Game",
            at(25.5, last, 144.0, 21.0),
        );
    });

    // SoundOptionsFrame, 400×285.
    let sound = dialog(
        &mut commands,
        &art,
        &images,
        f,
        s,
        Open::Sound,
        Vec2::new(400.0, 285.0),
        "Sound Options",
    );
    commands.entity(sound).with_children(|p| {
        for row in &SOUND_CHECKS {
            check_button(p, &art, f, row);
        }
        for row in &SOUND_SLIDERS {
            slider(p, &art, &images, f, s, row);
        }
        footer(p, &art, f);
    });

    // Interface Options: `BasicOptionsGeneral`'s mouse row and `BasicOptionsCamera`, boxed as there.
    let ui = dialog(
        &mut commands,
        &art,
        &images,
        f,
        s,
        Open::Interface,
        Vec2::new(400.0, 310.0),
        "Interface Options",
    );
    commands.entity(ui).with_children(|p| {
        option_box(
            p,
            &art,
            &images,
            f,
            s,
            "Controls",
            at(16.0, 52.0, 368.0, 60.0),
            |bx| {
                slider(bx, &art, &images, f, s, &MOUSE_SLIDER);
                check_button(bx, &art, f, &INVERT);
            },
        );
        option_box(
            p,
            &art,
            &images,
            f,
            s,
            "Camera",
            at(16.0, 132.0, 368.0, 120.0),
            |bx| {
                for row in &CAMERA_SLIDERS {
                    slider(bx, &art, &images, f, s, row);
                }
                dropdown(
                    bx,
                    &art,
                    &images,
                    f,
                    s,
                    Vec2::new(0.0, 24.0),
                    "Camera Following Style",
                );
            },
        );
        footer(p, &art, f);
    });
}

/// Defaults BOTTOMLEFT (16, 16); Cancel BOTTOMRIGHT (−16, 16) and Okay left of it; 100×21 each.
fn footer(p: &mut ChildSpawnerCommands, art: &Art, f: &Handle<Font>) {
    let node = |left: Option<f32>, right: Option<f32>| Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, px),
        right: right.map_or(Val::Auto, px),
        bottom: px(16.0),
        width: px(100.0),
        height: px(21.0),
        ..default()
    };
    panel_button(p, art, f, Act::Defaults, "Defaults", node(Some(16.0), None));
    panel_button(p, art, f, Act::Okay, "Okay", node(None, Some(116.0)));
    panel_button(p, art, f, Act::Cancel, "Cancel", node(None, Some(16.0)));
}

fn play(sounds: &mut Option<ResMut<Messages<GlueSound>>>, name: &'static str) {
    if let Some(s) = sounds {
        s.write(GlueSound(name));
    }
}

impl Panels {
    /// Open a sub-panel, keeping its CVars for Cancel.
    fn open_panel(&mut self, which: Open, cvars: &Cvars) {
        let rows: Vec<_> = match which {
            Open::Sound => sound_cvars().collect(),
            Open::Interface => interface_cvars().collect(),
            _ => Vec::new(),
        };
        self.initial = rows
            .into_iter()
            .map(|(name, default)| (name, cvars.get(name).unwrap_or(default).to_string()))
            .collect();
        self.open = which;
    }

    /// Cancel: every CVar back as it stood when the panel opened.
    fn cancel(&mut self, cvars: &mut Cvars) {
        for (name, value) in std::mem::take(&mut self.initial) {
            cvars.set(name, &value);
        }
        self.open = Open::GameMenu;
    }

    fn defaults(&self, cvars: &mut Cvars) {
        let rows: Vec<_> = match self.open {
            Open::Sound => sound_cvars().collect(),
            Open::Interface => interface_cvars().collect(),
            _ => Vec::new(),
        };
        for (name, default) in rows {
            cvars.set(name, default);
        }
    }
}

/// The menu opens with the pause and closes with it; Escape backs out one frame at a time, a
/// sub-panel's as its Cancel.
#[allow(clippy::too_many_arguments)]
pub(super) fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    mut next: ResMut<NextState<Phase>>,
    mut panels: ResMut<Panels>,
    cvars: Option<ResMut<Cvars>>,
    mut sounds: Option<ResMut<Messages<GlueSound>>>,
    mut lists: Query<(&mut Node, &mut Visibility), With<DropList>>,
) {
    if *phase.get() != Phase::Paused {
        panels.open = Open::None;
        return;
    }
    if panels.open == Open::None {
        panels.open = Open::GameMenu;
        play(&mut sounds, "igMainMenuOpen");
    }
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    // An open list closes first, as `CloseDropDownMenus` takes Escape before the frame does.
    let mut closed = false;
    for (mut node, mut vis) in &mut lists {
        if node.display != Display::None {
            node.display = Display::None;
            *vis = Visibility::Hidden;
            closed = true;
        }
    }
    if closed {
        return;
    }
    match panels.open {
        Open::Sound | Open::Interface => {
            if let Some(mut cvars) = cvars {
                panels.cancel(&mut cvars);
            }
            play(&mut sounds, "gsTitleOptionExit");
        }
        _ => {
            play(&mut sounds, "igMainMenuClose");
            next.set(Phase::Playing);
        }
    }
}

/// The buttons: the menu's own, and every panel's Okay, Cancel and Defaults.
#[allow(clippy::too_many_arguments)]
pub(super) fn clicks(
    presses: Query<(&Interaction, &Act), Changed<Interaction>>,
    mut next: ResMut<NextState<Phase>>,
    mut exit: MessageWriter<AppExit>,
    mut panels: ResMut<Panels>,
    cvars: Option<ResMut<Cvars>>,
    mut sounds: Option<ResMut<Messages<GlueSound>>>,
) {
    let Some(mut cvars) = cvars else { return };
    for (interaction, act) in &presses {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match act {
            Act::Sound => {
                play(&mut sounds, "igMainMenuOption");
                panels.open_panel(Open::Sound, &cvars);
            }
            Act::Interface => {
                play(&mut sounds, "igMainMenuOption");
                panels.open_panel(Open::Interface, &cvars);
            }
            Act::Logout => {
                play(&mut sounds, "igMainMenuLogout");
                next.set(Phase::Menu);
            }
            Act::Exit => {
                play(&mut sounds, "igMainMenuQuit");
                exit.write(AppExit::Success);
            }
            Act::Return => {
                play(&mut sounds, "igMainMenuContinue");
                next.set(Phase::Playing);
            }
            Act::Okay => {
                play(&mut sounds, "gsTitleOptionOK");
                panels.initial.clear();
                panels.open = Open::GameMenu;
            }
            Act::Cancel => {
                play(&mut sounds, "gsTitleOptionExit");
                panels.cancel(&mut cvars);
            }
            Act::Defaults => {
                play(&mut sounds, "igMainMenuOptionCheckBoxOn");
                panels.defaults(&mut cvars);
            }
        }
    }
}

/// Show the open frame, and only it; a hidden one leaves the layout so it takes no clicks.
pub(super) fn show(
    panels: Res<Panels>,
    mut roots: Query<(&Root, &mut Node, &mut Visibility)>,
    mut lists: Query<(&mut Node, &mut Visibility), (With<DropList>, Without<Root>)>,
) {
    if !panels.is_changed() {
        return;
    }
    for (root, mut node, mut vis) in &mut roots {
        let on = root.0 == panels.open && panels.open != Open::None;
        let display = if on { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
            *vis = if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
    if panels.open != Open::Interface {
        for (mut node, mut vis) in &mut lists {
            node.display = Display::None;
            *vis = Visibility::Hidden;
        }
    }
}

/// Hover and press art: the ADD highlight over whatever is under the cursor, the Pushed texture
/// while held.
#[allow(clippy::type_complexity)]
pub(super) fn hover(
    mut buttons: Query<
        (
            &Interaction,
            &Children,
            Option<&PanelButton>,
            Option<&Check>,
            &mut ImageNode,
        ),
        Changed<Interaction>,
    >,
    plain: Query<(&Interaction, &Children), (Changed<Interaction>, With<DropItem>)>,
    mut his: Query<&mut Visibility, With<Hi>>,
) {
    let mut light = |children: &Children, on: bool| {
        for c in children.iter() {
            if let Ok(mut v) = his.get_mut(c) {
                *v = if on {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    };
    for (interaction, children, button, check, mut img) in &mut buttons {
        light(children, *interaction != Interaction::None);
        let pressed = *interaction == Interaction::Pressed;
        let (up, down) = match (button, check) {
            (Some(b), _) => (&b.up, &b.down),
            (_, Some(c)) => (&c.up, &c.down),
            _ => continue,
        };
        let want = if pressed {
            down.as_ref().unwrap_or(up)
        } else {
            up
        };
        if img.image != *want {
            img.image = want.clone();
        }
    }
    for (interaction, children) in &plain {
        light(children, *interaction != Interaction::None);
    }
}

fn on(cvars: &Cvars, name: &str) -> bool {
    cvars
        .get(name)
        .is_some_and(|v| v.trim() != "0" && !v.trim().is_empty())
}

/// A click toggles the box's CVar; a dependant whose master is clear takes none.
pub(super) fn toggle_checks(
    presses: Query<(&Interaction, &Check), Changed<Interaction>>,
    cvars: Option<ResMut<Cvars>>,
    mut sounds: Option<ResMut<Messages<GlueSound>>>,
) {
    let Some(mut cvars) = cvars else { return };
    for (interaction, check) in &presses {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if check.needs.is_some_and(|n| !on(&cvars, n)) {
            continue;
        }
        let now = !on(&cvars, check.cvar);
        cvars.set(check.cvar, if now { "1" } else { "0" });
        play(
            &mut sounds,
            if now {
                "igMainMenuOptionCheckBoxOn"
            } else {
                "igMainMenuOptionCheckBoxOff"
            },
        );
    }
}

/// Dragging a slider sets its CVar at the step under the cursor.
pub(super) fn drag_sliders(
    sliders: Query<(&Interaction, &RelativeCursorPosition, &Slider)>,
    cvars: Option<ResMut<Cvars>>,
) {
    let Some(mut cvars) = cvars else { return };
    for (interaction, rel, s) in &sliders {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(n) = rel.normalized else { continue };
        // The thumb's centre travels the bar less half a thumb at each end.
        let x = (n.x + 0.5) * SLIDER_W;
        let frac = ((x - THUMB / 2.0) / (SLIDER_W - THUMB)).clamp(0.0, 1.0);
        let raw = s.min + frac * (s.max - s.min);
        let v = (s.min + ((raw - s.min) / s.step).round() * s.step).clamp(s.min, s.max);
        let text = format!("{v:.2}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        if cvars.get(s.cvar) != Some(text) {
            cvars.set(s.cvar, text);
        }
    }
}

/// The dropdown: the arrow opens the list, an entry picks it.
#[allow(clippy::type_complexity)]
pub(super) fn dropdown_clicks(
    arrows: Query<&Interaction, (Changed<Interaction>, With<DropButton>)>,
    items: Query<(&Interaction, &DropItem), Changed<Interaction>>,
    mut lists: Query<(&mut Node, &mut Visibility), With<DropList>>,
    cvars: Option<ResMut<Cvars>>,
    mut sounds: Option<ResMut<Messages<GlueSound>>>,
) {
    let Some(mut cvars) = cvars else { return };
    let mut toggle = arrows.iter().any(|i| *i == Interaction::Pressed);
    for (interaction, item) in &items {
        if *interaction == Interaction::Pressed {
            cvars.set(FOLLOW_CVAR, item.0);
            toggle = true;
        }
    }
    if !toggle {
        return;
    }
    play(&mut sounds, "igMainMenuOptionCheckBoxOn");
    for (mut node, mut vis) in &mut lists {
        let open = node.display == Display::None;
        node.display = if open { Display::Flex } else { Display::None };
        *vis = if open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Every control shows its CVar's value: checks, greyed dependants, thumbs, the dropdown.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn refresh(
    panels: Res<Panels>,
    cvars: Option<Res<Cvars>>,
    checks: Query<(&Check, &Children)>,
    mut marks: Query<(&CheckMark, &mut ImageNode, &mut Visibility)>,
    holders: Query<&Children, Without<Check>>,
    mut labels: Query<&mut TextColor, With<CheckLabel>>,
    sliders: Query<(&Slider, &Children)>,
    mut thumbs: Query<&mut Node, With<Thumb>>,
    mut drop_text: Query<&mut Text, With<DropText>>,
    mut drop_checks: Query<(&DropCheck, &mut Visibility), Without<CheckMark>>,
) {
    let Some(cvars) = cvars else { return };
    if panels.open == Open::None || !(cvars.is_changed() || panels.is_changed()) {
        return;
    }
    for (check, children) in &checks {
        let disabled = check.needs.is_some_and(|n| !on(&cvars, n));
        let checked = on(&cvars, check.cvar);
        for c in children.iter() {
            if let Ok((mark, mut img, mut vis)) = marks.get_mut(c) {
                let want = if checked {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *vis != want {
                    *vis = want;
                }
                let tex = match (&mark.dis, disabled) {
                    (Some(dis), true) => dis,
                    _ => &mark.on,
                };
                if img.image != *tex {
                    img.image = tex.clone();
                }
            }
            if let Ok(kids) = holders.get(c) {
                for k in kids.iter() {
                    if let Ok(mut color) = labels.get_mut(k) {
                        let want = if disabled { DISABLE } else { check.label_color };
                        if color.0 != want {
                            color.0 = want;
                        }
                    }
                }
            }
        }
    }
    for (s, children) in &sliders {
        let v = cvars.num(s.cvar).unwrap_or(s.min).clamp(s.min, s.max);
        let frac = (v - s.min) / (s.max - s.min).max(1e-6);
        for c in children.iter() {
            if let Ok(mut node) = thumbs.get_mut(c) {
                let left = px(frac * (SLIDER_W - THUMB));
                if node.left != left {
                    node.left = left;
                }
            }
        }
    }
    let style = cvars.get(FOLLOW_CVAR).unwrap_or("1").trim().to_string();
    let name = FOLLOW_STYLES
        .iter()
        .find(|(_, v)| *v == style)
        .map_or("Smart", |(n, _)| n);
    for mut t in &mut drop_text {
        if t.0 != name {
            t.0 = name.to_string();
        }
    }
    for (dc, mut vis) in &mut drop_checks {
        let want = if dc.0 == style {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}

/// The camera rows onto the rig, whenever the registry moves (and once at boot, from the saved
/// config): `mousespeed`, `cameraYawMoveSpeed`, `mouseInvertPitch`, `cameraSmoothStyle`,
/// `cameraYawSmoothSpeed` and `cameraDistanceMaxFactor`.
pub(super) fn apply_to_camera(cvars: Option<Res<Cvars>>, mut rig: ResMut<CamRig>) {
    let Some(cvars) = cvars else { return };
    if !cvars.is_changed() {
        return;
    }
    let num = |n: &str, d: f32| cvars.num(n).filter(|v| v.is_finite()).unwrap_or(d);
    let sensitivity = num("mousespeed", 1.0).clamp(0.5, 1.5);
    let look_speed = num("cameraYawMoveSpeed", 180.0).clamp(90.0, 270.0);
    let invert = on(&cvars, "mouseInvertPitch");
    let follow = match cvars.get(FOLLOW_CVAR).map(str::trim) {
        Some("0") | Some("3") => Follow::Never,
        Some("2") => Follow::Always,
        _ => Follow::Smart,
    };
    let follow_speed = num("cameraYawSmoothSpeed", 180.0).clamp(90.0, 270.0);
    let zoom_max = (num("cameraDistanceMaxFactor", 1.0).clamp(1.0, 2.0) * ZOOM_PER_FACTOR)
        .min(super::hero::ZOOM_MAX);
    let rig = &mut *rig;
    rig.sensitivity = sensitivity;
    rig.look_speed = look_speed;
    rig.invert_pitch = invert;
    rig.follow = follow;
    rig.follow_speed = follow_speed;
    rig.zoom_max = zoom_max;
}
