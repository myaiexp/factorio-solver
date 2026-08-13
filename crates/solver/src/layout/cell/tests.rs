// Test-module root for `size_step`. No `Grid` is involved anywhere below —
// sizing is arithmetic over rates and belt throughput, and `place`'s own
// tests cover what that arithmetic then builds.
//
// Split along the axis the sizing itself has: `sizing` is the single-product
// case the worked examples pin, `products` is what a second product changes.

mod products;
mod sizing;
