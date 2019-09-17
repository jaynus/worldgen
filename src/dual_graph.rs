use crate::HasValue;
use nalgebra::{Point2, Vector2};
use petgraph::{graph::NodeIndex, visit::EdgeRef};
use std::collections::HashMap;
use voronoi::voronoi;

pub type BorderNodeIdx = NodeIndex;
pub type BorderEdgeIdx = petgraph::graph::EdgeIndex;
pub type RegionNodeIdx = NodeIndex;
pub type RegionEdgeIdx = petgraph::graph::EdgeIndex;

#[derive(Debug)]
pub struct BorderNode<T = ()> {
    pub regions: Vec<RegionNodeIdx>,
    pub pos: Point2<f32>,
    pub value: T,
}
#[derive(Debug)]
pub struct BorderEdge {
    pub region_edge: Option<RegionEdgeIdx>,
    pub regions: Vec<RegionNodeIdx>,
}
#[derive(Debug)]
pub struct RegionNode<T = ()> {
    pub borders: Vec<BorderNodeIdx>,
    pub pos: Point2<f32>,
    pub value: T,
}

impl<T> HasValue for RegionNode<T> {
    type Value = T;

    fn value(&self) -> &Self::Value {
        &self.value
    }
    fn value_mut(&mut self) -> &mut Self::Value {
        &mut self.value
    }
}

impl<T> HasValue for BorderNode<T> {
    type Value = T;

    fn value(&self) -> &Self::Value {
        &self.value
    }
    fn value_mut(&mut self) -> &mut Self::Value {
        &mut self.value
    }
}

#[derive(Debug)]
pub struct RegionEdge {
    pub border_edge: Option<BorderEdgeIdx>,
    pub borders: Vec<BorderNodeIdx>,
}
pub type RegionGraph<T = ()> = petgraph::graph::UnGraph<RegionNode<T>, RegionEdge>;
pub type BorderGraph<T = ()> = petgraph::graph::UnGraph<BorderNode<T>, BorderEdge>;

fn poly_centroids(diagram: &voronoi::DCEL) -> Vec<voronoi::Point> {
    let mut face_centroids = vec![voronoi::Point::new(0.0, 0.0); diagram.faces.len()];
    let mut num_face_vertices = vec![0; diagram.faces.len()];
    for edge in &diagram.halfedges {
        if !edge.alive {
            continue;
        }
        let pt = diagram.vertices[edge.origin].coordinates;
        let face_pt = face_centroids[edge.face];
        face_centroids[edge.face] = voronoi::Point::new(
            face_pt.x.into_inner() + pt.x.into_inner(),
            face_pt.y.into_inner() + pt.y.into_inner(),
        );
        num_face_vertices[edge.face] += 1;
    }
    for i in 0..num_face_vertices.len() {
        let num_vertices = num_face_vertices[i];
        let face_pt = face_centroids[i];
        face_centroids[i] = voronoi::Point::new(
            face_pt.x.into_inner() / f64::from(num_vertices),
            face_pt.y.into_inner() / f64::from(num_vertices),
        );
    }

    face_centroids.remove(face_centroids.len() - 1);
    face_centroids
}

fn contains(point: &voronoi::Point, points: &[voronoi::Point]) -> bool {
    let epsilon = 0.001;

    for v in points {
        if (point.x.into_inner() - v.x.into_inner()).abs() < epsilon
            && (point.y.into_inner() - v.y.into_inner()).abs() < epsilon
        {
            return true;
        }
    }

    false
}

fn gen_points<R>(count: usize, bounds: &voronoi::Point, rng: &mut R) -> Vec<voronoi::Point>
where
    R: rand::Rng + ?Sized,
{
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let mut point = {
            let x: f64 = rng.sample(rand::distributions::Standard);
            let y: f64 = rng.sample(rand::distributions::Standard);
            voronoi::Point::new(x * bounds.x.into_inner(), y * bounds.y.into_inner())
        };

        while contains(&point, &points) {
            point = {
                let x: f64 = rng.sample(rand::distributions::Standard);
                let y: f64 = rng.sample(rand::distributions::Standard);
                voronoi::Point::new(x * bounds.x.into_inner(), y * bounds.y.into_inner())
            };
        }
        points.push(point);
    }

    points.sort_unstable_by_key(|p| p.y);

    if points.len() >= 2 {
        let last = points.len() - 1;
        let secondlast = points.len() - 2;

        let epsilon = 0.001;
        let biggest = points[last].y.into_inner();
        let maybe_also_biggest = points[secondlast].y.into_inner();
        if (biggest - maybe_also_biggest) < epsilon {
            points[last].y = (biggest + epsilon).into();
        }
    }

    points
}

fn gen_voronoi_delaunay<R>(
    dims: voronoi::Point,
    num_points: usize,
    num_lloyd_iterations: u32,
    rng: &mut R,
) -> (
    Vec<delaunator::Point>,
    Vec<delaunator::Point>,
    Vec<Vec<delaunator::Point>>,
)
where
    R: rand::Rng + ?Sized,
{
    let mut points: Vec<_> = gen_points(num_points, &dims, rng)
        .iter()
        .map(|p| delaunator::Point { x: *p.x, y: *p.y })
        .collect();
    let mut triangulation;
    let mut i = 0;
    let mut voronoi_points;
    let mut voronoi_polys;
    loop {
        triangulation = delaunator::triangulate(points.as_slice()).unwrap();
        voronoi_points = Vec::new();
        voronoi_polys = Vec::new();
        let mut triangles_seen = vec![false; triangulation.triangles.len()];
        // the tri_iter defines the point from which we start to iterate through
        'triangle_loop: for (tri_iter, _) in triangulation.triangles.iter().enumerate() {
            let mut curr_tri = tri_iter;
            // if the halfedge has already been visited or the halfedge is from the hull, continue
            if triangles_seen[curr_tri] || curr_tri == delaunator::EMPTY {
                continue;
            }
            let mut num_poly_points = 0;
            let mut poly_centroid = delaunator::Point { x: 0.0, y: 0.0 };
            let mut vor_poly_points = Vec::new();
            loop {
                triangles_seen[curr_tri] = true;
                let pt_idx = triangulation.triangles[curr_tri];
                let a = &points[pt_idx];
                curr_tri = delaunator::next_halfedge(curr_tri);
                let pt_idx = triangulation.triangles[curr_tri];
                let b = &points[pt_idx];
                curr_tri = delaunator::next_halfedge(curr_tri);
                let pt_idx = triangulation.triangles[curr_tri];
                let c = &points[pt_idx];
                let x_sum = a.x + b.x + c.x;
                let y_sum = a.y + b.y + c.y;
                // a vertex in the voronoi polygon
                let voronoi_pt = delaunator::Point {
                    x: x_sum / 3.0,
                    y: y_sum / 3.0,
                };
                // calculate the poly centroid
                poly_centroid.x += voronoi_pt.x;
                poly_centroid.y += voronoi_pt.y;
                num_poly_points += 1;
                let next_tri = triangulation.halfedges[curr_tri];
                // if the poly has a point from the hull, bail
                if next_tri == delaunator::EMPTY {
                    continue 'triangle_loop;
                }
                // only produce poly points if we are not doing a lloyd iteration
                if i == num_lloyd_iterations {
                    vor_poly_points.push(voronoi_pt.clone());
                }
                // done when we are back at the starting halfedge
                if next_tri == tri_iter {
                    break;
                }
                curr_tri = next_tri;
            }
            voronoi_points.push(delaunator::Point {
                x: poly_centroid.x / num_poly_points as f64,
                y: poly_centroid.y / num_poly_points as f64,
            });
            if i == num_lloyd_iterations {
                voronoi_polys.push(vor_poly_points);
            }
        }
        if i == num_lloyd_iterations {
            break;
        }
        // during lloyd relaxation, put hull points into the point set
        for hull_pt in triangulation.hull {
            voronoi_points.push(points[hull_pt].clone());
        }
        points = voronoi_points;
        // points = poly_centroids(&vor_diagram);
        i += 1;
    }
    (points, voronoi_points, voronoi_polys)
}

fn gen_voronoi<R>(
    dims: voronoi::Point,
    num_points: usize,
    num_lloyd_iterations: u32,
    rng: &mut R,
) -> voronoi::DCEL
where
    R: rand::Rng + ?Sized,
{
    let points = gen_points(num_points, &dims, rng);
    let mut vor_diagram;
    let mut points: Vec<voronoi::Point> = points;
    let mut i = 0;
    loop {
        vor_diagram = voronoi(&points, dims.x.into());
        if i == num_lloyd_iterations {
            break;
        }
        points = poly_centroids(&vor_diagram);
        i += 1;
    }
    vor_diagram
}

#[allow(clippy::cast_possible_truncation)]
fn get_or_insert_border_node<T>(
    border_node_map: &mut HashMap<usize, BorderNodeIdx>,
    graph: &mut BorderGraph<T>,
    diagram: &voronoi::DCEL,
    idx: usize,
) -> BorderNodeIdx
where
    T: Default,
{
    if let Some(border_node) = border_node_map.get(&idx) {
        *border_node
    } else {
        let pos = diagram.vertices[idx].coordinates;

        let border_node = graph.add_node(BorderNode {
            regions: Vec::new(),
            pos: Point2::new(pos.x.into_inner() as f32, pos.y.into_inner() as f32),
            value: Default::default(),
        });
        border_node_map.insert(idx, border_node);
        border_node
    }
}
fn get_or_insert_region_node<T>(
    region_node_map: &mut HashMap<usize, RegionNodeIdx>,
    graph: &mut RegionGraph<T>,
    pos: Point2<f32>,
    idx: usize,
) -> RegionNodeIdx
where
    T: Default,
{
    if let Some(region_node) = region_node_map.get(&idx) {
        *region_node
    } else {
        let region_node = graph.add_node(RegionNode {
            borders: Vec::new(),
            pos,
            value: Default::default(),
        });
        region_node_map.insert(idx, region_node);
        region_node
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn gen_dual_graph<RN, BN, R>(
    dims: Vector2<f32>,
    num_points: usize,
    num_lloyd_iterations: u32,
    rng: &mut R,
) -> (RegionGraph<RN>, BorderGraph<BN>)
where
    R: rand::Rng + ?Sized,
    RN: Default,
    BN: Default,
{
    let vor_diagram = gen_voronoi(
        voronoi::Point::new(f64::from(dims.x), f64::from(dims.y)),
        num_points,
        num_lloyd_iterations,
        rng,
    );

    let mut region_graph = RegionGraph::<RN>::new_undirected();
    let mut border_graph = BorderGraph::<BN>::new_undirected();
    let mut border_node_map: HashMap<usize, BorderNodeIdx> = HashMap::new();
    let mut region_node_map: HashMap<usize, RegionNodeIdx> = HashMap::new();
    for (i, face) in vor_diagram
        .faces
        .iter()
        .take(vor_diagram.faces.len() - 1)
        .enumerate()
    {
        let region_node_idx = get_or_insert_region_node(
            &mut region_node_map,
            &mut region_graph,
            Point2::new(0.0, 0.0),
            i,
        );
        let region_node = &mut region_graph[region_node_idx];
        let mut curr_edge = face.outer_component;
        let mut prev_edge;
        let mut pos = Point2::new(0.0, 0.0);
        let mut num_edges = 0;
        loop {
            prev_edge = curr_edge;
            curr_edge = vor_diagram.halfedges[curr_edge].next;
            let border_idx = get_or_insert_border_node(
                &mut border_node_map,
                &mut border_graph,
                &vor_diagram,
                vor_diagram.halfedges[curr_edge].origin,
            );
            region_node.borders.push(border_idx);
            let next_idx = get_or_insert_border_node(
                &mut border_node_map,
                &mut border_graph,
                &vor_diagram,
                vor_diagram.halfedges[prev_edge].origin,
            );
            let edge_idx =
                if let Some((e, _)) = border_graph.find_edge_undirected(border_idx, next_idx) {
                    e
                } else {
                    border_graph.add_edge(
                        border_idx,
                        next_idx,
                        BorderEdge {
                            region_edge: None,
                            regions: Vec::new(),
                        },
                    )
                };
            border_graph[edge_idx].regions.push(region_node_idx);
            num_edges += 1;
            let vertex_pos =
                vor_diagram.vertices[vor_diagram.halfedges[curr_edge].origin].coordinates;
            pos = Point2::new(
                pos.x + vertex_pos.x.into_inner() as f32,
                pos.y + vertex_pos.y.into_inner() as f32,
            );
            if curr_edge == face.outer_component {
                break;
            }
        }
        region_node.pos = pos / num_edges as f32;
    }

    for edge in border_graph.edge_references() {
        let regions = &edge.weight().regions;
        if regions.len() > 1 {
            assert!(regions.len() == 2);
            let region_a = regions[0];
            let region_b = regions[1];
            if region_graph
                .find_edge_undirected(region_a, region_b)
                .is_none()
            {
                region_graph.add_edge(
                    region_a,
                    region_b,
                    RegionEdge {
                        border_edge: Some(edge.id()),
                        borders: vec![edge.source(), edge.target()],
                    },
                );
            }
        }
    }
    for edge in region_graph.edge_references() {
        let borders = &edge.weight().borders;
        assert!(borders.len() == 2);
        let border_a = borders[0];
        let border_b = borders[1];
        let (edge_idx, _) = border_graph
            .find_edge_undirected(border_a, border_b)
            .expect("border edge did not exist");
        let border_edge = &mut border_graph[edge_idx];
        border_edge.region_edge.replace(edge.id());
    }
    (region_graph, border_graph)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    pub fn gen_voronoi_delaunay_test() {
        let dims = voronoi::Point {
            x: 1024.0.into(),
            y: 1024.0.into(),
        };
        let mut rng =
            rand_xorshift::XorShiftRng::from_seed([1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4]);
        let (seed_points, voronoi_points, voronoi_polys) =
            gen_voronoi_delaunay::<rand_xorshift::XorShiftRng>(dims, 100, 0, &mut rng);
        let mut imgbuf = image::ImageBuffer::from_pixel(
            *dims.x as u32,
            *dims.y as u32,
            image::Rgb([222, 222, 222]),
        );
        for pt in &voronoi_points {
            imageproc::drawing::draw_filled_circle_mut(
                &mut imgbuf,
                (pt.x as i32, pt.y as i32),
                2,
                image::Rgb([222, 0, 0]),
            );
        }
        for pt in &seed_points {
            imageproc::drawing::draw_filled_circle_mut(
                &mut imgbuf,
                (pt.x as i32, pt.y as i32),
                2,
                image::Rgb([0, 222, 0]),
            );
        }
        for poly in &voronoi_polys {
            for i in 1..poly.len() {
                let start = &poly[i - 1];
                let end = &poly[i % poly.len()];
                imageproc::drawing::draw_antialiased_line_segment_mut(
                    &mut imgbuf,
                    (start.x as i32, start.y as i32),
                    (end.x as i32, end.y as i32),
                    image::Rgb([0, 0, 222]),
                    imageproc::pixelops::interpolate,
                );
            }
        }
        imgbuf.save("output/delaunay_test.png").unwrap();
    }

    #[test]
    pub fn gen_dual_graph_test() {
        let dims = Vector2::new(1024.0, 1024.0);
        let mut imgbuf = image::ImageBuffer::from_pixel(
            dims.x as u32,
            dims.y as u32,
            image::Rgb([222, 222, 222]),
        );

        let mut rng = rand_xorshift::XorShiftRng::from_seed([
            122, 154, 21, 182, 159, 131, 187, 243, 134, 230, 110, 10, 31, 174, 6, 4,
        ]);

        let (region_graph, border_graph) =
            gen_dual_graph::<(), (), rand_xorshift::XorShiftRng>(dims, 8000, 2, &mut rng);
        draw_graph(
            &mut imgbuf,
            &region_graph,
            |n| (image::Rgb([0, 0, 222]), n.pos, 2),
            |e| {
                use petgraph::visit::EdgeRef;
                let source_node = &region_graph[e.source()];
                let target_node = &region_graph[e.target()];
                (image::Rgb([0, 0, 222]), source_node.pos, target_node.pos)
            },
        );
        draw_graph(
            &mut imgbuf,
            &border_graph,
            |n| (image::Rgb([222, 0, 0]), n.pos, 2),
            |e| {
                use petgraph::visit::EdgeRef;
                let source_node = &border_graph[e.source()];
                let target_node = &border_graph[e.target()];
                (image::Rgb([0, 222, 0]), source_node.pos, target_node.pos)
            },
        );
        for edge in border_graph.edge_references() {
            use petgraph::visit::EdgeRef;
            let regions = &edge.weight().regions;
            let pos_a = border_graph[edge.source()].pos;
            let pos_b = border_graph[edge.target()].pos;
            let pos = Point2::from((pos_a.coords + pos_b.coords) / 2.0);

            for region in regions.iter() {
                let pos_region = region_graph[*region].pos;
                imageproc::drawing::draw_antialiased_line_segment_mut(
                    &mut imgbuf,
                    (pos.x as i32, pos.y as i32),
                    (pos_region.x as i32, pos_region.y as i32),
                    image::Rgb([0, 222, 222]),
                    imageproc::pixelops::interpolate,
                );
            }
        }
        for edge in region_graph.edge_references() {
            use petgraph::visit::EdgeRef;
            let borders = &edge.weight().borders;
            let pos_a = region_graph[edge.source()].pos;
            let pos_b = region_graph[edge.target()].pos;
            let pos = Point2::from((pos_a.coords + pos_b.coords) / 2.0);

            for border in borders.iter() {
                let pos_border = border_graph[*border].pos;
                imageproc::drawing::draw_antialiased_line_segment_mut(
                    &mut imgbuf,
                    (pos.x as i32, pos.y as i32),
                    (pos_border.x as i32, pos_border.y as i32),
                    image::Rgb([0, 222, 222]),
                    imageproc::pixelops::interpolate,
                );
            }
        }
        imgbuf.save("output/graphs.png").unwrap();
    }

    pub(crate) fn draw_graph<
        G: petgraph::visit::IntoNodeReferences + petgraph::visit::IntoEdgeReferences,
        N: Fn(
            &<G as petgraph::visit::Data>::NodeWeight,
        ) -> (<I as image::GenericImageView>::Pixel, Point2<f32>, i32),
        E: Fn(
            <G as petgraph::visit::IntoEdgeReferences>::EdgeRef,
        ) -> (
            <I as image::GenericImageView>::Pixel,
            Point2<f32>,
            Point2<f32>,
        ),
        I,
    >(
        imgbuf: &mut I,
        graph: G,
        node_color: N,
        edge_color: E,
    ) where
        I: image::GenericImage,
        I::Pixel: 'static,
        <<I as image::GenericImageView>::Pixel as image::Pixel>::Subpixel:
            conv::ValueInto<f32> + imageproc::definitions::Clamp<f32>,
    {
        use petgraph::visit::NodeRef;
        for node in graph.node_references() {
            let (color, pt, size) = node_color(node.weight());
            imageproc::drawing::draw_filled_circle_mut(
                imgbuf,
                (pt.x as i32, pt.y as i32),
                size,
                color,
            );
        }
        for edge in graph.edge_references() {
            let (color, from, to) = edge_color(edge);
            imageproc::drawing::draw_antialiased_line_segment_mut(
                imgbuf,
                (from.x as i32, from.y as i32),
                (to.x as i32, to.y as i32),
                color,
                imageproc::pixelops::interpolate,
            );
        }
    }
}
