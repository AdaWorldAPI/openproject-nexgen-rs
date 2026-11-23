#!/usr/bin/env ruby
# frozen_string_literal: true

# Oracle for `zammad_parity::feed::float_text` (tests/float_text_oracle.rs).
#
# Prints one line per float: the f64 bit pattern as 16 lowercase hex digits,
# a tab, and Ruby's `Float#to_s` of that float. The Rust test reads
# float_to_s.tsv and needs no Ruby at test time.
#
# Regenerate (from the repository root, with the Ruby the adapter runs on):
#
#   ruby crates/zammad-parity/tests/fixtures/float_to_s.rb \
#     > crates/zammad-parity/tests/fixtures/float_to_s.tsv
#
# The set is fixed and deterministic (fixed-seed generator), so a regeneration
# on the same Ruby changes nothing.

seen = {}
out = []
add = lambda do |f|
  next unless f.is_a?(Float) && f.finite?

  bits = [f].pack('E').unpack1('Q<')
  next if seen[bits]

  seen[bits] = true
  out << format("%016x\t%s", bits, f.to_s)
end

# The cases the review named.
[667_962_307_169_479.25, 1_000_000_000_000_001.0, 1e15, 1e16,
 123_456_789_012_345.6, 0.1 + 0.2, 0.0, -0.0, 1.0, -1.0, 0.5, 100.0, 1234.5,
 0.0001, 0.00001, 0.001234, 123_456_789.123_456_79, 1e100, -1e-100, 2.5e-300,
 9_007_199_254_740_993.0, 4_503_599_627_370_496.5].each { |f| add.call(f) }

# Powers of ten and their neighbours.
(-20..25).each do |k|
  f = Float("1e#{k}")
  [f, f.next_float, f.prev_float, -f].each { |g| add.call(g) }
end

# Extremes and subnormals.
[Float::MAX, -Float::MAX, Float::MIN, Float::EPSILON, 5e-324, -5e-324,
 2.225073858507201e-308, 1e-310, 1e-320].each { |f| add.call(f) }
(1..40).each { |i| add.call([i].pack('Q<').unpack1('E')) }
(0..3).each { |i| add.call([(1 << 52) - 1 - i].pack('Q<').unpack1('E')) }

# Exact decimal ties: halves of an ulp between two shortest candidates.
rng = Random.new(20_261_123)
300.times do
  m = rng.rand((1 << 46)...(1 << 51))
  [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875].each { |frac| add.call(m + frac) }
end
300.times do
  m = rng.rand((1 << 40)...(1 << 52))
  add.call(m + 0.5)
  add.call((m + 0.5) / 8.0)
  add.call((m + 0.5) * 8.0)
end

# Random finite bit patterns, and random "ordinary" decimals.
count = 0
while count < 1200
  f = [rng.bytes(8).unpack1('Q<')].pack('Q<').unpack1('E')
  next unless f.finite?

  add.call(f)
  count += 1
end
600.times { add.call(rng.rand * 10.0**rng.rand(-6..18)) }

puts out
