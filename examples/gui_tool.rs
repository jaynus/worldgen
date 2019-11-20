use imgui::*;
use nalgebra::Vector2;
use std::collections::HashMap;
mod support;
use glium::Texture2d;
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;
use std::rc::Rc;
use worldgen::*;

#[derive(Default)]
struct Pixel {
    elevation: f32,
}
impl HasElevation<f32> for Pixel {
    fn elevation(&self) -> f32 {
        self.elevation
    }
    fn set_elevation(&mut self, height: f32) {
        self.elevation = height;
    }
}

pub struct DualGraphSettings {
    num_points: i32,
    num_lloyd_reduction: i32,
}
impl Default for DualGraphSettings {
    fn default() -> Self {
        Self {
            num_points: 9000,
            num_lloyd_reduction: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum PeakDistribution {
    CenteredRandom,
}

pub struct PeakAutomataSettings {
    num_peaks: usize,
    distribution: PeakDistribution,
}
impl Default for PeakAutomataSettings {
    fn default() -> Self {
        Self {
            peaks: 20,
            distribution: PeakDistribution::CenteredRandom,
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreviewImage {
    DualGraph,
    HeightMap,
    SimpleWinds,
}

pub struct GuiState {
    texture_map: HashMap<PreviewImage, TextureId>,
    seed: ImString,
    dual_graph_settings: DualGraphSettings,
    peak_automata_settings: PeakAutomataSettings,
    rng: XorShiftRng,
    dimensions: [f32; 2],
}
impl GuiState {
    fn set_seed(&mut self, seed_str: &str) {
        self.seed = ImString::new(seed_str);

        let seed = seed_from_string(self.seed.to_str());
        self.rng = XorShiftRng::from_seed(seed);
    }
}
impl Default for GuiState {
    fn default() -> Self {
        let seed = seed_from_string("balls");
        Self {
            texture_map: HashMap::default(),
            seed: ImString::new("balls"),
            dual_graph_settings: DualGraphSettings::default(),
            dimensions: [1024.0, 1024.0],
            rng: XorShiftRng::from_seed(seed),
        }
    }
}
impl GuiState {
    pub fn gen_dual_graph(&mut self, textures: &mut Textures<Rc<Texture2d>>) {
        if let Some(texture_id) = self.texture_map.get(&PreviewImage::DualGraph) {
            textures.remove(*texture_id);
        }

        let seed = self.seed.to_string();
        self.set_seed(seed.as_str());

        let (region_graph, border_graph) = dual_graph::gen_dual_graph::<Pixel, (), rand_xorshift::XorShiftRng>(
            Vector2::new(self.dimensions[0], self.dimensions[1]),
            self.dual_graph_settings.num_points as usize,
            self.dual_graph_settings.num_lloyd_reduction as u32,
            &mut self.rng,
        );
        log::trace!("Generated");
    }
}

fn build_window(ui: &mut Ui<'_>, state: &mut GuiState, textures: &mut Textures<Rc<Texture2d>>) {
    Window::new(im_str!("Worldgen Preview Tool"))
        .size([1024.0, 768.0], Condition::Always)
        .position([0.0, 0.0], Condition::Always)
        .resizable(false)
        .movable(false)
        .flags(WindowFlags::NO_TITLE_BAR)
        .build(ui, || {
            InputText::new(ui, im_str!("Seed"), &mut state.seed).build();
            InputFloat2::new(ui, im_str!("Width"), &mut state.dimensions).build();

            if CollapsingHeader::new(ui, im_str!("Phase 1 - Daul Graph")).default_open(true).build() {
                InputInt::new(ui, im_str!("Lloyd Reductions"), &mut state.dual_graph_settings.num_lloyd_reduction).build();
                InputInt::new(ui, im_str!("Points"), &mut state.dual_graph_settings.num_points)
                    .step(100)
                    .build();

                if ui.button(im_str!("Generate"), [0.0, 0.0]) {
                    state.gen_dual_graph(textures);
                }
            }
            if ui.collapsing_header(im_str!("Phase 2 - Heightmap")).default_open(true).build() {
                InputInt::new(ui, im_str!("Peaks"), &mut state.peak_automata_settings.num_peaks).build();
            }
            //preview
        });
}

fn main() {
    let system = support::init(file!());
    let mut gui_state = GuiState::default();

    system.main_loop(|_, renderer, ui| {
        build_window(ui, &mut gui_state, renderer.textures());
    });
}

fn seed_from_string(seed: &str) -> [u8; 16] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.input(seed.as_bytes());

    let mut seed: [u8; 16] = [0; 16];
    seed.copy_from_slice(&hasher.result()[0..16]);
    seed
}
