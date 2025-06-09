// Copyright (c) Meta Platforms, Inc. and affiliates.

// Coord system:
//  -1,-1     0, 1       1,1
//
//  -1,0      0, 0       1,0
//
//  -1,-1     0,-1      1,-1

use glam::Vec2;

#[derive(Clone, Copy)]
enum QuadIndex {
    TopLeft = 0,
    TopRight = 1,
    BottomLeft = 2,
    BottomRight = 3,
}

type GraphNodeIDX = usize;

// Root node position is 0 and we will also use it as None value
// because root node can't be a child of another node.
type QuadNodeIDX = usize;

#[derive(Clone, Copy, Debug)]
pub struct BHGraphNode {
    pub idx: GraphNodeIDX,
    pub position: Vec2,
    pub mass: f32,
}

#[derive(Clone, Debug)]
pub struct QuadNode {
    center_position: Vec2,
    // size of the side of the square that contains this quad node
    size: f32,
    children: [QuadNodeIDX; 4],
    graph_nodes: Vec<BHGraphNode>,
    center_of_mass: Vec2,
    total_mass: f32,
    depth: usize,
}

impl QuadNode {
    fn is_leaf(&self) -> bool {
        self.children == [0, 0, 0, 0]
    }
}

pub struct QuadTree {
    pub bodies: usize,
    pub quad_nodes: Vec<QuadNode>,
    max_depth: usize,
}

impl QuadTree {
    pub fn new(max_depth: usize) -> Self {
        QuadTree {
            quad_nodes: vec![],
            bodies: 0,
            max_depth,
        }
    }

    pub fn compute_forces(&self) -> Vec<Vec2> {
        let mut forces = vec![Vec2::ZERO; self.bodies];

        for quad_node in &self.quad_nodes {
            if !quad_node.is_leaf() {
                continue;
            }

            for graph_node in &quad_node.graph_nodes {
                forces[graph_node.idx] = self.compute_force_for_node(graph_node);
            }
        }

        forces
    }

    fn compute_force_for_node(&self, graph_node: &BHGraphNode) -> Vec2 {
        const THETA: f32 = 0.9; // threshold for using quad node

        let mut stack = vec![0]; // start from root

        let mut total_force = Vec2::ZERO;
        while let Some(quad_node_idx) = stack.pop() {
            let quad_node = &self.quad_nodes[quad_node_idx];
            if quad_node.is_leaf() {
                total_force += force_exerted_on_graph_node(graph_node, quad_node);
            } else {
                let distance_between_graph_node_and_quad_node_center_of_mass =
                    ((quad_node.center_of_mass.x - graph_node.position.x).powi(2)
                        + (quad_node.center_of_mass.y - graph_node.position.y).powi(2))
                    .sqrt();

                let should_use_quad_node = (quad_node.size
                    / distance_between_graph_node_and_quad_node_center_of_mass)
                    < THETA;

                if should_use_quad_node {
                    total_force += force_exerted_on_graph_node(graph_node, quad_node);
                } else {
                    if quad_node.children[QuadIndex::TopLeft as usize] != 0 {
                        stack.push(quad_node.children[QuadIndex::TopLeft as usize]);
                    }
                    if quad_node.children[QuadIndex::TopRight as usize] != 0 {
                        stack.push(quad_node.children[QuadIndex::TopRight as usize]);
                    }
                    if quad_node.children[QuadIndex::BottomLeft as usize] != 0 {
                        stack.push(quad_node.children[QuadIndex::BottomLeft as usize]);
                    }
                    if quad_node.children[QuadIndex::BottomRight as usize] != 0 {
                        stack.push(quad_node.children[QuadIndex::BottomRight as usize]);
                    }
                }
            }
        }

        total_force
    }

    pub fn add_body(&mut self, graph_node: BHGraphNode) {
        self.bodies += 1;
        let graph_node_position = graph_node.position;

        if self.quad_nodes.is_empty() {
            self.quad_nodes.push(QuadNode {
                center_position: Vec2 { x: 0.0, y: 0.0 },
                size: 2.0,
                children: [0, 0, 0, 0],
                graph_nodes: vec![graph_node],
                center_of_mass: graph_node.position,
                total_mass: graph_node.mass,
                depth: 0,
            });
            return;
        }

        let mut quad_node_idx = 0; // start from root

        loop {
            let current_quad_node_len = self.quad_nodes.len();
            let quad_node = &mut self.quad_nodes[quad_node_idx];

            if quad_node.is_leaf() {
                if quad_node.depth >= self.max_depth {
                    // this node is a leaf and we can't split it anymore
                    // so we can just add the body to it
                    quad_node.graph_nodes.push(graph_node);
                    add_mass_to_quad_node(quad_node, graph_node);
                    break;
                } else {
                    //  split the node and then we'll go through it in the
                    // next iteration
                    self.split_quad_node(quad_node_idx);
                }
            } else {
                let quad_index = get_quad_index(graph_node_position, quad_node.center_position);
                let child_node_idx = quad_node.children[quad_index as usize];

                if child_node_idx == 0 {
                    // child node doesn't exist yet. We can put our body in here.

                    add_mass_to_quad_node(quad_node, graph_node);

                    let child_size = quad_node.size / 2.0;
                    let child_center_position =
                        new_quad_center(quad_index, quad_node.center_position, quad_node.size);
                    let new_quad_node = QuadNode {
                        center_position: child_center_position,
                        size: child_size,
                        children: [0, 0, 0, 0],
                        graph_nodes: vec![graph_node],
                        center_of_mass: graph_node.position,
                        total_mass: graph_node.mass,
                        depth: quad_node.depth + 1,
                    };
                    let child_quad_node_idx = current_quad_node_len;
                    quad_node.children[quad_index as usize] = child_quad_node_idx;
                    self.quad_nodes.push(new_quad_node);
                    break;
                } else {
                    // update the center of mass and total mass for this node
                    add_mass_to_quad_node(quad_node, graph_node);

                    // and then go deeper
                    quad_node_idx = child_node_idx;
                }
            }
        }
    }

    fn split_quad_node(&mut self, quad_node_idx: QuadNodeIDX) {
        let (child, quad_idx) = {
            let parent = &mut self.quad_nodes[quad_node_idx];

            let child_size = parent.size / 2.0;
            if parent.graph_nodes.len() != 1 {
                panic!("We can only split a leaf quad node with a single graph node.");
            }
            let graph_node = parent.graph_nodes[0];

            let quad_idx = get_quad_index(graph_node.position, parent.center_position);
            let child_center_position =
                new_quad_center(quad_idx, parent.center_position, parent.size);

            let child = QuadNode {
                center_position: child_center_position,
                size: child_size,
                children: [0, 0, 0, 0],
                graph_nodes: vec![graph_node],
                center_of_mass: graph_node.position,
                total_mass: graph_node.mass,
                depth: parent.depth + 1,
            };
            (child, quad_idx)
        };

        self.quad_nodes.push(child);
        let child_idx = self.quad_nodes.len() - 1;

        let parent = &mut self.quad_nodes[quad_node_idx];
        parent.graph_nodes = vec![];
        parent.children[quad_idx as usize] = child_idx;
    }
}

fn force_exerted_on_graph_node(graph_node: &BHGraphNode, quad_node: &QuadNode) -> Vec2 {
    const EPSILON: f32 = 0.01;

    const G: f32 = 0.000000025;

    let distance = quad_node.center_of_mass.distance(graph_node.position);

    let dx = quad_node.center_of_mass.x - graph_node.position.x;
    let dy = quad_node.center_of_mass.y - graph_node.position.y;

    let mut force_magnitude = (quad_node.total_mass / (distance + EPSILON)) * G;

    // If they're too close make them hella repel each other
    // so they don't become a singularity
    if distance < EPSILON / 2.0 {
        force_magnitude *= 100.0;
    }

    Vec2 {
        x: force_magnitude * dx,
        y: force_magnitude * dy,
    }
}

fn add_mass_to_quad_node(quad_node: &mut QuadNode, graph_node: BHGraphNode) {
    let new_total_mass = quad_node.total_mass + graph_node.mass;

    quad_node.center_of_mass.x = (quad_node.center_of_mass.x * (quad_node.total_mass)
        + graph_node.position.x * graph_node.mass)
        / new_total_mass;

    quad_node.center_of_mass.y = (quad_node.center_of_mass.y * (quad_node.total_mass)
        + graph_node.position.y * graph_node.mass)
        / new_total_mass;

    quad_node.total_mass = new_total_mass;
}

fn get_quad_index(node_position: Vec2, center_position: Vec2) -> QuadIndex {
    match (
        node_position.x <= center_position.x,
        node_position.y <= center_position.y,
    ) {
        (true, false) => QuadIndex::TopLeft,
        (false, false) => QuadIndex::TopRight,
        (true, true) => QuadIndex::BottomLeft,
        (false, true) => QuadIndex::BottomRight,
    }
}

fn new_quad_center(quad_index: QuadIndex, center_position: Vec2, size: f32) -> Vec2 {
    let offset = size / 4.0;
    match quad_index {
        QuadIndex::TopLeft => Vec2 {
            x: center_position.x - offset,
            y: center_position.y + offset,
        },
        QuadIndex::TopRight => Vec2 {
            x: center_position.x + offset,
            y: center_position.y + offset,
        },
        QuadIndex::BottomLeft => Vec2 {
            x: center_position.x - offset,
            y: center_position.y - offset,
        },
        QuadIndex::BottomRight => Vec2 {
            x: center_position.x + offset,
            y: center_position.y - offset,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_graph() -> Vec<BHGraphNode> {
        let nodes = [
            (0.0, 0.0),
            (1.0, 1.0),
            (-1.0, -1.0),
            (0.5, 0.0),
            (0.0, 0.5),
            (0.1, 0.1),
            (0.2, 0.2),
        ];
        nodes
            .into_iter()
            .enumerate()
            .map(|(idx, (x, y))| BHGraphNode {
                idx,
                position: Vec2 { x, y },
                mass: 1.0,
            })
            .collect()
    }

    #[test]
    fn barnes_hut() {
        let nodes = make_test_graph();
        let mut quad_tree = QuadTree::new(2);
        for graph_node in nodes {
            quad_tree.add_body(graph_node);
        }

        let mut result = String::new();
        for (idx, quad_node) in quad_tree.quad_nodes.iter().enumerate() {
            result.push_str(&format!("{idx} {quad_node:#?}\n\n  ---\n\n"));
        }

        k9::snapshot!(
            result,
            "
0 QuadNode {
    center_position: Vec2(
        0.0,
        0.0,
    ),
    size: 2.0,
    children: [
        6,
        2,
        1,
        5,
    ],
    graph_nodes: [],
    center_of_mass: Vec2(
        0.114285715,
        0.114285715,
    ),
    total_mass: 7.0,
    depth: 0,
}

  ---

1 QuadNode {
    center_position: Vec2(
        -0.5,
        -0.5,
    ),
    size: 1.0,
    children: [
        0,
        3,
        4,
        0,
    ],
    graph_nodes: [],
    center_of_mass: Vec2(
        -0.5,
        -0.5,
    ),
    total_mass: 2.0,
    depth: 1,
}

  ---

2 QuadNode {
    center_position: Vec2(
        0.5,
        0.5,
    ),
    size: 1.0,
    children: [
        0,
        7,
        8,
        0,
    ],
    graph_nodes: [],
    center_of_mass: Vec2(
        0.43333337,
        0.43333337,
    ),
    total_mass: 3.0,
    depth: 1,
}

  ---

3 QuadNode {
    center_position: Vec2(
        -0.25,
        -0.25,
    ),
    size: 0.5,
    children: [
        0,
        0,
        0,
        0,
    ],
    graph_nodes: [
        BHGraphNode {
            idx: 0,
            position: Vec2(
                0.0,
                0.0,
            ),
            mass: 1.0,
        },
    ],
    center_of_mass: Vec2(
        0.0,
        0.0,
    ),
    total_mass: 1.0,
    depth: 2,
}

  ---

4 QuadNode {
    center_position: Vec2(
        -0.75,
        -0.75,
    ),
    size: 0.5,
    children: [
        0,
        0,
        0,
        0,
    ],
    graph_nodes: [
        BHGraphNode {
            idx: 2,
            position: Vec2(
                -1.0,
                -1.0,
            ),
            mass: 1.0,
        },
    ],
    center_of_mass: Vec2(
        -1.0,
        -1.0,
    ),
    total_mass: 1.0,
    depth: 2,
}

  ---

5 QuadNode {
    center_position: Vec2(
        0.5,
        -0.5,
    ),
    size: 1.0,
    children: [
        0,
        0,
        0,
        0,
    ],
    graph_nodes: [
        BHGraphNode {
            idx: 3,
            position: Vec2(
                0.5,
                0.0,
            ),
            mass: 1.0,
        },
    ],
    center_of_mass: Vec2(
        0.5,
        0.0,
    ),
    total_mass: 1.0,
    depth: 1,
}

  ---

6 QuadNode {
    center_position: Vec2(
        -0.5,
        0.5,
    ),
    size: 1.0,
    children: [
        0,
        0,
        0,
        0,
    ],
    graph_nodes: [
        BHGraphNode {
            idx: 4,
            position: Vec2(
                0.0,
                0.5,
            ),
            mass: 1.0,
        },
    ],
    center_of_mass: Vec2(
        0.0,
        0.5,
    ),
    total_mass: 1.0,
    depth: 1,
}

  ---

7 QuadNode {
    center_position: Vec2(
        0.75,
        0.75,
    ),
    size: 0.5,
    children: [
        0,
        0,
        0,
        0,
    ],
    graph_nodes: [
        BHGraphNode {
            idx: 1,
            position: Vec2(
                1.0,
                1.0,
            ),
            mass: 1.0,
        },
    ],
    center_of_mass: Vec2(
        1.0,
        1.0,
    ),
    total_mass: 1.0,
    depth: 2,
}

  ---

8 QuadNode {
    center_position: Vec2(
        0.25,
        0.25,
    ),
    size: 0.5,
    children: [
        0,
        0,
        0,
        0,
    ],
    graph_nodes: [
        BHGraphNode {
            idx: 5,
            position: Vec2(
                0.1,
                0.1,
            ),
            mass: 1.0,
        },
        BHGraphNode {
            idx: 6,
            position: Vec2(
                0.2,
                0.2,
            ),
            mass: 1.0,
        },
    ],
    center_of_mass: Vec2(
        0.15,
        0.15,
    ),
    total_mass: 2.0,
    depth: 2,
}

  ---


"
        );

        let forces = quad_tree.compute_forces();
        k9::snapshot!(
            forces,
            "
[
    Vec2(
        5.8273507e-8,
        5.8273507e-8,
    ),
    Vec2(
        -1.0349679e-7,
        -1.0349679e-7,
    ),
    Vec2(
        1.0480374e-7,
        1.0480373e-7,
    ),
    Vec2(
        -9.545608e-8,
        3.662312e-8,
    ),
    Vec2(
        3.662312e-8,
        -9.545607e-8,
    ),
    Vec2(
        3.2199164e-8,
        3.2199164e-8,
    ),
    Vec2(
        -4.135354e-8,
        -4.1353545e-8,
    ),
]
"
        );
    }
}
