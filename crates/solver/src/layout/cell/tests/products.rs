// What a second product changes about sizing: a product claims a whole belt
// rather than a lane, so the product side's belt count — not its lane count —
// is what a two-product step has to fit into.
use super::super::*;

use crate::testsupport::{blue_belt, green_circuit_plan, hand_step, plan_containing_kovarex, rate, step};

/// `uranium-processing` yields two items from one ore. The default topology
/// puts products on the 1-belt edge, and one belt can't give two items each
/// their own — the refusal names both products and the belt count.
#[test]
fn two_products_on_a_one_belt_side_is_too_many_products() {
    let plan = plan_containing_kovarex();
    let processing = step(&plan, "uranium-processing");
    match size_step(processing, blue_belt(), &CellTopology::default()) {
        Err(LayoutError::TooManyProductsForBelts { recipe, products, belts }) => {
            assert_eq!(recipe, "uranium-processing");
            assert_eq!(products.len(), 2);
            assert!(products.contains(&"uranium-235".to_string()));
            assert!(products.contains(&"uranium-238".to_string()));
            assert_eq!(belts, 1);
        }
        other => panic!("expected TooManyProductsForBelts, got {other:?}"),
    }
}

/// Moving products onto the 2-belt spine (`ingredients_on: Edge`) gives
/// `uranium-processing`'s two outputs a belt each — a whole belt, not a
/// lane, so `product_belts` for each item is a single distinct slot.
#[test]
fn two_products_split_the_product_belts() {
    let plan = plan_containing_kovarex();
    let processing = step(&plan, "uranium-processing");
    let topo = CellTopology { ingredients_on: Side::Edge, ..CellTopology::default() };
    let sized = size_step(processing, blue_belt(), &topo).unwrap();

    assert_eq!(sized.product_allocation.len(), 2);
    assert!(
        sized.product_allocation.iter().all(|(_, belts)| *belts == 1),
        "two products over two belts: one each, {:?}",
        sized.product_allocation
    );
    let belts_235 = sized.product_belts("uranium-235");
    let belts_238 = sized.product_belts("uranium-238");
    assert_eq!(belts_235.len(), 1);
    assert_eq!(belts_238.len(), 1);
    assert_ne!(belts_235[0], belts_238[0], "each product must own a distinct belt slot");
}

/// A hand-built two-product step where the product side binds the cell
/// (a tiny ingredient rate against ample lanes) — `plan_containing_kovarex`'s
/// own ratios don't happen to hit this case: uranium-processing consumes 10
/// ore per craft against yielding ~1 item total, so on any topology this
/// design can express (at most 2 belts a side) the ore *ingredient* binds
/// first, which is correct behaviour, just not the case this test is for.
/// Here uranium-238 is the ~142x larger of the two product streams
/// (probability 0.993 vs 0.007), so it alone achieves the column cap;
/// uranium-235's much smaller rate has ample headroom on its own belt and
/// is never named, and neither is the (deliberately tiny) ingredient.
#[test]
fn bound_by_names_the_larger_product() {
    let hstep = hand_step(
        "uranium-processing",
        1,
        vec![rate("uranium-ore", 0.001)],
        vec![rate("uranium-235", 0.0007), rate("uranium-238", 0.0993)],
    );
    let topo = CellTopology { ingredients_on: Side::Edge, ..CellTopology::default() };
    let sized = size_step(&hstep, blue_belt(), &topo).unwrap();

    assert!(sized.bound_by.contains("uranium-238"), "bound_by = {:?}", sized.bound_by);
    assert!(!sized.bound_by.contains("uranium-235"), "bound_by = {:?}", sized.bound_by);
    assert!(!sized.bound_by.contains("uranium-ore"), "bound_by = {:?}", sized.bound_by);
}

/// A single-product step is unaffected by the product side now allocating
/// belts instead of assuming one product owns all of them: it still gets
/// every product belt, and its `machines_per_cell` is unchanged from the
/// pre-multi-product value pinned in `worked_example_express_belts`.
#[test]
fn a_single_product_still_takes_every_product_belt() {
    let plan = green_circuit_plan();
    let topo = CellTopology::default();
    let circuit = step(&plan, "electronic-circuit");
    let sized = size_step(circuit, blue_belt(), &topo).unwrap();

    assert_eq!(sized.product_allocation, vec![("electronic-circuit".to_string(), topo.product_belts())]);
    assert_eq!(
        sized.product_belts("electronic-circuit"),
        (0..topo.product_belts()).collect::<Vec<u32>>()
    );
    assert_eq!(sized.machines_per_cell, 15);
}
