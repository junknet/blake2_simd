use crate::*;
use core::mem;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const MAX_DEGREE: usize = 4;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub const MAX_DEGREE: usize = 4;

// Variants other than Portable are unreachable in no_std, unless CPU features
// are explicitly enabled for the build with e.g. RUSTFLAGS="-C target-feature=avx2".
// This might change in the future if is_x86_feature_detected moves into libcore.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Platform {
    Portable,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    SSE41,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    AVX2,
}

#[derive(Clone, Copy, Debug)]
pub struct Implementation(Platform);

impl Implementation {
    pub fn detect() -> Self {
        // Try the different implementations in order of how fast/modern they
        // are. Currently on non-x86, everything just uses portable.
        if let Some(avx2_impl) = Self::avx2_if_supported() {
            avx2_impl
        } else if let Some(sse41_impl) = Self::sse41_if_supported() {
            sse41_impl
        } else {
            Self::portable()
        }
    }

    pub fn portable() -> Self {
        Implementation(Platform::Portable)
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[allow(unreachable_code)]
    pub fn sse41_if_supported() -> Option<Self> {
        // Check whether SSE4.1 support is assumed by the build.
        #[cfg(target_feature = "sse4.1")]
        {
            return Some(Implementation(Platform::SSE41));
        }
        // Otherwise dynamically check for support if we can.
        #[cfg(feature = "std")]
        {
            if is_x86_feature_detected!("sse4.1") {
                return Some(Implementation(Platform::SSE41));
            }
        }
        None
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[allow(unreachable_code)]
    pub fn avx2_if_supported() -> Option<Self> {
        // Check whether AVX2 support is assumed by the build.
        #[cfg(target_feature = "avx2")]
        {
            return Some(Implementation(Platform::AVX2));
        }
        // Otherwise dynamically check for support if we can.
        #[cfg(feature = "std")]
        {
            if is_x86_feature_detected!("avx2") {
                return Some(Implementation(Platform::AVX2));
            }
        }
        None
    }

    pub fn degree(&self) -> usize {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 => 4,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::SSE41 => 2,
            Platform::Portable => 1,
        }
    }

    pub fn compress1_loop(
        &self,
        state: &mut u64x8,
        input: &[u8],
        count: u128,
        last_block: u64,
        last_node: u64,
        blocks: usize,
        stride: usize,
        buffer_tail: usize,
    ) {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 => unsafe {
                avx2::compress1_loop(
                    state,
                    input,
                    count,
                    last_block,
                    last_node,
                    blocks,
                    stride,
                    buffer_tail,
                );
            },
            // Note that there's an SSE version of compress1 in the official C
            // implementation, but I haven't ported it yet.
            _ => {
                portable::compress1_loop(
                    state,
                    input,
                    count,
                    last_block,
                    last_node,
                    blocks,
                    stride,
                    buffer_tail,
                );
            }
        }
    }

    pub fn compress1_loop_b(&self, job: Job, stride: Stride) {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 => unsafe {
                avx2::compress1_loop_b(job, stride);
            },
            // Note that there's an SSE version of compress1 in the official C
            // implementation, but I haven't ported it yet.
            _ => {
                portable::compress1_loop_b(job, stride);
            }
        }
    }

    pub fn compress2_loop(
        &self,
        state0: &mut u64x8,
        state1: &mut u64x8,
        input0: &[u8],
        input1: &[u8],
        count_low: &u64x2,
        count_high: &u64x2,
        last_block: &u64x2,
        last_node: &u64x2,
        blocks: usize,
        stride: usize,
        buffer_tail: &u64x2,
    ) {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 | Platform::SSE41 => unsafe {
                sse41::compress2_loop(
                    state0,
                    state1,
                    input0,
                    input1,
                    count_low,
                    count_high,
                    last_block,
                    last_node,
                    blocks,
                    stride,
                    buffer_tail,
                );
            },
            Platform::Portable => {
                self.compress1_loop(
                    state0,
                    input0,
                    count_low[0] as u128 + ((count_high[0] as u128) << 64),
                    last_block[0],
                    last_node[0],
                    blocks,
                    stride,
                    buffer_tail[0] as usize,
                );
                self.compress1_loop(
                    state1,
                    input1,
                    count_low[1] as u128 + ((count_high[1] as u128) << 64),
                    last_block[1],
                    last_node[1],
                    blocks,
                    stride,
                    buffer_tail[1] as usize,
                );
            }
        }
    }

    pub fn compress2_loop_b(&self, jobs: &mut [Job; 2], stride: Stride) {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 | Platform::SSE41 => unsafe { sse41::compress2_loop_b(jobs, stride) },
            _ => panic!("unsupported"),
        }
    }

    pub fn compress4_loop(
        &self,
        state0: &mut u64x8,
        state1: &mut u64x8,
        state2: &mut u64x8,
        state3: &mut u64x8,
        input0: &[u8],
        input1: &[u8],
        input2: &[u8],
        input3: &[u8],
        count_low: &u64x4,
        count_high: &u64x4,
        last_block: &u64x4,
        last_node: &u64x4,
        blocks: usize,
        stride: usize,
        buffer_tail: &u64x4,
    ) {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 => unsafe {
                avx2::compress4_loop(
                    state0,
                    state1,
                    state2,
                    state3,
                    input0,
                    input1,
                    input2,
                    input3,
                    count_low,
                    count_high,
                    last_block,
                    last_node,
                    blocks,
                    stride,
                    buffer_tail,
                );
            },
            _ => {
                // Performance note: Would it be faster to add a compress4_loop
                // interface to the sse41 implementation, which used a single
                // loop to process the inputs together instead of one after the
                // other? For BLAKE2bp it probably would be, because you'll get
                // better cache performance by traversing the input once
                // instead of twice. But for tree hashes probably not, since
                // the inputs are usually adjacent or nearly-adjacent rather
                // than overlapping. Tree hash performance is our priority
                // here, and also doing things this way is simpler.
                self.compress2_loop(
                    state0,
                    state1,
                    input0,
                    input1,
                    &count_low.split()[0],
                    &count_high.split()[0],
                    &last_block.split()[0],
                    &last_node.split()[0],
                    blocks,
                    stride,
                    &buffer_tail.split()[0],
                );
                self.compress2_loop(
                    state2,
                    state3,
                    input2,
                    input3,
                    &count_low.split()[1],
                    &count_high.split()[1],
                    &last_block.split()[1],
                    &last_node.split()[1],
                    blocks,
                    stride,
                    &buffer_tail.split()[1],
                );
            }
        }
    }

    pub fn compress4_loop_b(&self, jobs: &mut [Job; 4], stride: Stride) {
        match self.0 {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Platform::AVX2 => unsafe { avx2::compress4_loop_b(jobs, stride) },
            _ => panic!("unsupported"),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C, align(16))]
pub struct u64x2(pub [u64; 2]);

impl core::ops::Deref for u64x2 {
    type Target = [u64; 2];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for u64x2 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C, align(32))]
pub struct u64x4(pub [u64; 4]);

impl u64x4 {
    #[inline(always)]
    pub(crate) fn split(&self) -> &[u64x2; 2] {
        // Safety note: The 32-byte alignment of u64x4 guarantees that each
        // half of it will be 16-byte aligned, and the C repr guarantees that
        // the layout is exactly four packed u64's.
        unsafe { mem::transmute(self) }
    }

    #[inline(always)]
    pub(crate) fn split_mut(&mut self) -> &mut [u64x2; 2] {
        // Safety note: The 32-byte alignment of u64x4 guarantees that each
        // half of it will be 16-byte aligned, and the C repr guarantees that
        // the layout is exactly four packed u64's.
        unsafe { mem::transmute(self) }
    }
}

impl core::ops::Deref for u64x4 {
    type Target = [u64; 4];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for u64x4 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C, align(64))]
pub struct u64x8(pub [u64; 8]);

impl u64x8 {
    #[inline(always)]
    pub(crate) fn split(&self) -> &[u64x4; 2] {
        // Safety note: The 64-byte alignment of u64x8 guarantees that each
        // half of it will be 32-byte aligned, and the C repr guarantees that
        // the layout is exactly eight packed u64's.
        unsafe { mem::transmute(self) }
    }

    #[inline(always)]
    pub(crate) fn split_mut(&mut self) -> &mut [u64x4; 2] {
        // Safety note: The 64-byte alignment of u64x8 guarantees that each
        // half of it will be 32-byte aligned, and the C repr guarantees that
        // the layout is exactly eight packed u64's.
        unsafe { mem::transmute(self) }
    }
}

impl core::ops::Deref for u64x8 {
    type Target = [u64; 8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for u64x8 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stride {
    // e.g. BLAKE2b
    Normal,
    // e.g. BLAKE2bp
    Parallel,
}

impl Stride {
    #[inline(always)]
    pub fn padded_blockbytes(&self) -> usize {
        match self {
            Stride::Normal => BLOCKBYTES,
            Stride::Parallel => crate::blake2bp::DEGREE * BLOCKBYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finalize {
    NotYet,
    YesOrdinary,
    YesLastNode,
}

impl Finalize {
    #[inline(always)]
    pub fn last_block_flag(&self) -> bool {
        match self {
            Finalize::NotYet => false,
            _ => true,
        }
    }

    #[inline(always)]
    pub fn last_node_flag(&self) -> bool {
        match self {
            Finalize::YesLastNode => true,
            _ => false,
        }
    }
}

pub struct Job<'a, 'b> {
    pub words: &'a mut u64x8,
    pub count: u128,
    pub input: &'b [u8],
    pub finalize: Finalize,
    // The constructor contains debug asserts, so include a private field to
    // force callers to use it.
    _use_the_constructor_please: (),
}

impl<'a, 'b> Job<'a, 'b> {
    pub fn new(
        words: &'a mut u64x8,
        count: u128,
        input: &'b [u8],
        finalize: Finalize,
    ) -> Job<'a, 'b> {
        if let Finalize::NotYet = finalize {
            // Only the very last block is allowed to be shorter than
            // BLOCKBYTES, so if we're not finalizing yet, the input must be an
            // even multiple of BLOCKBYTES.
            debug_assert_eq!(0, input.len() % BLOCKBYTES);
        }
        Job {
            words,
            count,
            input,
            finalize,
            _use_the_constructor_please: (),
        }
    }

    #[inline(always)]
    pub fn offset(&mut self, offset: usize) {
        let start = cmp::min(self.input.len(), offset);
        self.input = &self.input[start..];
        self.count = self.count.wrapping_add(offset as u128);
    }
}

// Note that even an empty input has a final block at offset 0, which will wind
// up being all zeros.
#[inline(always)]
pub(crate) fn final_block_offset(min_len: usize, stride: Stride) -> usize {
    let final_byte = min_len.saturating_sub(1);
    final_byte - (final_byte % stride.padded_blockbytes())
}

// Returns (block, len).
#[inline(always)]
pub(crate) fn get_block<'a>(
    input: &'a [u8],
    offset: usize,
    buffer: &'a mut [u8; BLOCKBYTES],
    stride: Stride,
) -> (&'a [u8; BLOCKBYTES], usize, bool) {
    debug_assert!(BLOCKBYTES < u8::max_value() as usize);
    debug_assert!(offset == 0 || offset < input.len());
    let start = cmp::min(offset, input.len());
    let is_end = (input.len() - start) <= stride.padded_blockbytes();
    let len = cmp::min(BLOCKBYTES, input.len() - start);
    if input.len() - start >= BLOCKBYTES {
        (array_ref!(input, start, BLOCKBYTES), BLOCKBYTES, is_end)
    } else {
        buffer[..len].copy_from_slice(&input[start..][..len]);
        (buffer, len, is_end)
    }
}

#[inline(always)]
pub(crate) fn u64_flag(flag: bool) -> u64 {
    if flag {
        !0
    } else {
        0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_detection() {
        assert_eq!(Platform::Portable, Implementation::portable().0);

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        #[cfg(feature = "std")]
        {
            if is_x86_feature_detected!("avx2") {
                assert_eq!(Platform::AVX2, Implementation::detect().0);
                assert_eq!(
                    Platform::AVX2,
                    Implementation::avx2_if_supported().unwrap().0
                );
                assert_eq!(
                    Platform::SSE41,
                    Implementation::sse41_if_supported().unwrap().0
                );
            } else if is_x86_feature_detected!("sse4.1") {
                assert_eq!(Platform::SSE41, Implementation::detect().0);
                assert!(Implementation::avx2_if_supported().is_none());
                assert_eq!(
                    Platform::SSE41,
                    Implementation::sse41_if_supported().unwrap().0
                );
            } else {
                assert_eq!(Platform::Portable, Implementation::detect().0);
                assert!(Implementation::avx2_if_supported().is_none());
                assert!(Implementation::sse41_if_supported().is_none());
            }
        }
    }

    fn input_state_words(i: u64) -> u64x8 {
        let mut words = u64x8([0; 8]);
        for j in 0..words.len() {
            words[j] = i + j as u64;
        }
        words
    }

    fn exercise_cases<F>(mut f: F)
    where
        F: FnMut(usize, usize, u128, bool, bool, usize, usize),
    {
        // Chose counts to hit the relevant overflow cases.
        let counts = &[
            0u128,
            (1u128 << 64) - BLOCKBYTES as u128,
            0u128.wrapping_sub(BLOCKBYTES as u128),
        ];
        for invocations in 1..=2 {
            for blocks_per_invoc in 1..=3 {
                for &count in counts {
                    for &last_block in &[true, false] {
                        for &last_node in &[true, false] {
                            for stride in 1..=3 {
                                for &buffer_tail in &[0, 1, BLOCKBYTES - 1, BLOCKBYTES] {
                                    if invocations * blocks_per_invoc != 1
                                        && buffer_tail == BLOCKBYTES
                                    {
                                        // Skip the empty block case when there's more than a single
                                        // block of input. We have asserts preventing that.
                                        continue;
                                    }
                                    // eprintln!("\ncase -----");
                                    // dbg!(invocations);
                                    // dbg!(blocks_per_invoc);
                                    // dbg!(count);
                                    // dbg!(last_block);
                                    // dbg!(last_node);
                                    // dbg!(stride);
                                    // dbg!(buffer_tail);
                                    f(
                                        invocations,
                                        blocks_per_invoc,
                                        count,
                                        last_block,
                                        last_node,
                                        stride,
                                        buffer_tail,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn exercise_cases_b<F>(mut f: F)
    where
        F: FnMut(usize, usize, u128, Finalize, Stride, usize),
    {
        // Chose counts to hit the relevant overflow cases.
        let counts = &[
            0u128,
            (1u128 << 64) - BLOCKBYTES as u128,
            0u128.wrapping_sub(BLOCKBYTES as u128),
        ];
        for invocations in 1..=2 {
            for blocks_per_invoc in 1..=3 {
                for &count in counts {
                    for &finalize in &[
                        Finalize::NotYet,
                        Finalize::YesOrdinary,
                        Finalize::YesLastNode,
                    ] {
                        for &stride in &[Stride::Normal, Stride::Parallel] {
                            for &buffer_tail in &[0, 1, BLOCKBYTES - 1, BLOCKBYTES] {
                                // eprintln!("\ncase -----");
                                // dbg!(invocations);
                                // dbg!(blocks_per_invoc);
                                // dbg!(count);
                                // dbg!(finalize);
                                // dbg!(stride);
                                // dbg!(buffer_tail);

                                // Skip the empty block case when there's more
                                // than a single block of input. It's not
                                // really valid, and our test reference doesn't
                                // do the right thing either.
                                if invocations * blocks_per_invoc != 1 && buffer_tail == BLOCKBYTES
                                {
                                    continue;
                                }
                                // Skip non-zero buffer tails when not
                                // finalizing. We assert against doing that.
                                if let Finalize::NotYet = finalize {
                                    if buffer_tail != 0 {
                                        continue;
                                    }
                                }

                                f(
                                    invocations,
                                    blocks_per_invoc,
                                    count,
                                    finalize,
                                    stride,
                                    buffer_tail,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // For various loop lengths and finalization parameters, make sure that the
    // implementation gives the same answer as the portable implementation does
    // when invoked one block at a time. (So even the portable implementation
    // itself is being tested here, to make sure its loop is correct.) Note
    // that this doesn't include any fixed test vectors; those are taken from
    // the blake2-kat.json file (copied from upstream) and tested elsewhere.
    fn exercise_compress1_loop(implementation: Implementation) {
        let mut input = [0; 100 * BLOCKBYTES];
        paint_test_input(&mut input);
        exercise_cases(
            |invocations, blocks_per_invoc, count, last_block, last_node, stride, buffer_tail| {
                // Use the portable implementation, one block at a time, to
                // compute the final state that we expect.
                let mut reference_state = input_state_words(0);
                for block in 0..invocations * blocks_per_invoc {
                    let input_block = array_ref!(&input, block * stride * BLOCKBYTES, BLOCKBYTES);
                    let is_last_block = block == invocations * blocks_per_invoc - 1;
                    let maybe_tail = if is_last_block { buffer_tail } else { 0 };
                    portable::compress1_loop(
                        &mut reference_state,
                        input_block,
                        count.wrapping_add((block * BLOCKBYTES) as u128),
                        u64_flag(is_last_block && last_block),
                        u64_flag(is_last_block && last_node),
                        1, // blocks, one at a time
                        stride,
                        maybe_tail,
                    );
                }

                // Do the same thing with the implementation
                // under test, and make sure they're the same.
                let mut test_state = input_state_words(0);
                for invocation in 0..invocations {
                    let is_last_invoc = invocation == invocations - 1;
                    let maybe_tail = if is_last_invoc { buffer_tail } else { 0 };
                    implementation.compress1_loop(
                        &mut test_state,
                        &input[invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        count.wrapping_add((invocation * blocks_per_invoc * BLOCKBYTES) as u128),
                        u64_flag(is_last_invoc && last_block),
                        u64_flag(is_last_invoc && last_node),
                        blocks_per_invoc,
                        stride,
                        maybe_tail,
                    );
                }
                assert_eq!(reference_state, test_state);
            },
        );
    }
    fn exercise_compress1_loop_b(implementation: Implementation) {
        let mut input = [0; 100 * BLOCKBYTES];
        paint_test_input(&mut input);
        exercise_cases_b(
            |invocations, blocks_per_invoc, count, finalize, stride, buffer_tail| {
                // Use the portable implementation, one block at a time, to
                // compute the final state that we expect.
                let mut reference_words = input_state_words(0);
                let mut reference_count = count;
                let total_blocks = invocations * blocks_per_invoc;
                for block in 0..total_blocks {
                    let input_block =
                        array_ref!(&input, block * stride.padded_blockbytes(), BLOCKBYTES);
                    let is_last_block = block == total_blocks - 1;
                    let input_slice = if is_last_block {
                        &input_block[..BLOCKBYTES - buffer_tail]
                    } else {
                        &input_block[..]
                    };
                    let maybe_finalize = if !is_last_block {
                        Finalize::NotYet
                    } else {
                        finalize
                    };
                    portable::compress1_loop_b(
                        Job::new(
                            &mut reference_words,
                            reference_count,
                            input_slice,
                            maybe_finalize,
                        ),
                        stride,
                    );
                    reference_count = reference_count.wrapping_add(BLOCKBYTES as u128);
                }

                // Do the same thing in batches with the implementation under
                // test, and make sure they're the same.
                let mut test_words = input_state_words(0);
                let mut test_count = count;
                for invoc_num in 0..invocations {
                    let is_last_invoc = invoc_num == invocations - 1;
                    let offset = invoc_num * blocks_per_invoc * stride.padded_blockbytes();
                    let mut len = blocks_per_invoc * stride.padded_blockbytes();
                    if is_last_invoc {
                        // Buffer tail cuts into the input block itself, not into its stride padding.
                        len = len - stride.padded_blockbytes() + BLOCKBYTES - buffer_tail;
                    }
                    let input_slice = &input[offset..][..len];
                    let maybe_finalize = if !is_last_invoc {
                        Finalize::NotYet
                    } else {
                        finalize
                    };
                    implementation.compress1_loop_b(
                        Job::new(&mut test_words, test_count, input_slice, maybe_finalize),
                        stride,
                    );
                    test_count = test_count.wrapping_add((blocks_per_invoc * BLOCKBYTES) as u128);
                }
                assert_eq!(reference_words, test_words);
            },
        );
    }

    #[test]
    fn test_compress1_loop_portable() {
        exercise_compress1_loop(Implementation::portable());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress1_loop_sse41() {
        // Currently this just falls back to portable, but we test it anyway.
        if let Some(imp) = Implementation::sse41_if_supported() {
            exercise_compress1_loop(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress1_loop_avx2() {
        if let Some(imp) = Implementation::avx2_if_supported() {
            exercise_compress1_loop(imp);
        }
    }

    #[test]
    fn test_compress1_loop_portable_b() {
        exercise_compress1_loop_b(Implementation::portable());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress1_loop_sse41_b() {
        // Currently this just falls back to portable, but we test it anyway.
        if let Some(imp) = Implementation::sse41_if_supported() {
            exercise_compress1_loop_b(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress1_loop_avx2_b() {
        if let Some(imp) = Implementation::avx2_if_supported() {
            exercise_compress1_loop_b(imp);
        }
    }

    // Similar to exercise_compress1_loop above.
    fn exercise_compress2_loop(implementation: Implementation) {
        let mut input_buffer = [0; 100 * BLOCKBYTES];
        paint_test_input(&mut input_buffer);
        let inputs = [&input_buffer[0..], &input_buffer[1..]];
        exercise_cases(
            |invocations, blocks_per_invoc, count, last_block, last_node, stride, buffer_tail| {
                // Use the portable compress1_loop implementation to compute a
                // reference state for each input separately.
                let mut reference_states = [input_state_words(0), input_state_words(1)];
                for i in 0..reference_states.len() {
                    portable::compress1_loop(
                        &mut reference_states[i],
                        inputs[i],
                        count,
                        u64_flag(last_block),
                        u64_flag(last_node),
                        invocations * blocks_per_invoc,
                        stride,
                        buffer_tail,
                    );
                }

                // Do the same thing in parallel with the
                // implementation under test under test, and
                // make sure the result is the same.
                let mut test_state0 = input_state_words(0);
                let mut test_state1 = input_state_words(1);
                for invocation in 0..invocations {
                    let is_last_invoc = invocation == invocations - 1;
                    let invoc_count =
                        count.wrapping_add((invocation * blocks_per_invoc * BLOCKBYTES) as u128);
                    let count_low = u64x2([invoc_count as u64; 2]);
                    let count_high = u64x2([(invoc_count >> 64) as u64; 2]);
                    let last_block = u64x2([u64_flag(is_last_invoc && last_block); 2]);
                    let last_node = u64x2([u64_flag(is_last_invoc && last_node); 2]);
                    let maybe_tail = u64x2([if is_last_invoc { buffer_tail as u64 } else { 0 }; 2]);
                    implementation.compress2_loop(
                        &mut test_state0,
                        &mut test_state1,
                        &inputs[0][invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        &inputs[1][invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        &count_low,
                        &count_high,
                        &last_block,
                        &last_node,
                        blocks_per_invoc,
                        stride,
                        &maybe_tail,
                    );
                }
                assert_eq!(reference_states[0], test_state0);
                assert_eq!(reference_states[1], test_state1);
            },
        );
    }

    fn exercise_compress2_loop_b(implementation: Implementation) {
        let mut input_buffer = [0; 100 * BLOCKBYTES];
        paint_test_input(&mut input_buffer);
        let inputs = [&input_buffer[0..], &input_buffer[1..]];
        exercise_cases_b(
            |invocations, blocks_per_invoc, count, finalize, stride, buffer_tail| {
                // Use the portable compress1_loop implementation to compute a
                // reference state for each input separately.
                let total_blocks = invocations * blocks_per_invoc;
                let mut len = total_blocks * stride.padded_blockbytes();
                if buffer_tail != 0 {
                    len = len - stride.padded_blockbytes() + BLOCKBYTES - buffer_tail;
                }
                let mut reference_words = [input_state_words(0), input_state_words(1)];
                for i in 0..reference_words.len() {
                    portable::compress1_loop_b(
                        Job::new(&mut reference_words[i], count, &inputs[i][..len], finalize),
                        stride,
                    );
                }

                // Do the same thing with the implementation under test under
                // test, and make sure the result is the same.
                let mut test_words = [input_state_words(0), input_state_words(1)];
                let mut test_count = count;
                for invoc_num in 0..invocations {
                    let is_last_invoc = invoc_num == invocations - 1;
                    let offset = invoc_num * blocks_per_invoc * stride.padded_blockbytes();
                    let mut len = blocks_per_invoc * stride.padded_blockbytes();
                    if is_last_invoc && buffer_tail != 0 {
                        len = len - stride.padded_blockbytes() + BLOCKBYTES - buffer_tail;
                    }
                    let maybe_finalize = if !is_last_invoc {
                        Finalize::NotYet
                    } else {
                        finalize
                    };
                    let &mut [ref mut words0, ref mut words1] = &mut test_words;
                    let mut jobs = [
                        Job::new(
                            words0,
                            test_count,
                            &inputs[0][offset..][..len],
                            maybe_finalize,
                        ),
                        Job::new(
                            words1,
                            test_count,
                            &inputs[1][offset..][..len],
                            maybe_finalize,
                        ),
                    ];
                    implementation.compress2_loop_b(&mut jobs, stride);
                    test_count = test_count.wrapping_add((blocks_per_invoc * BLOCKBYTES) as u128);
                    for job in &jobs {
                        assert!(job.input.is_empty());
                    }
                }
                assert_eq!(reference_words, test_words);
            },
        );
    }

    #[test]
    fn test_compress2_loop_portable() {
        exercise_compress2_loop(Implementation::portable());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress2_loop_sse41() {
        if let Some(imp) = Implementation::sse41_if_supported() {
            exercise_compress2_loop(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress2_loop_avx2() {
        // Currently this just falls back to SSE4.1, but we test it anyway.
        if let Some(imp) = Implementation::avx2_if_supported() {
            exercise_compress2_loop(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress2_loop_sse41_b() {
        if let Some(imp) = Implementation::sse41_if_supported() {
            exercise_compress2_loop_b(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress2_loop_avx2_b() {
        // Currently this just falls back to SSE4.1, but we test it anyway.
        if let Some(imp) = Implementation::avx2_if_supported() {
            exercise_compress2_loop_b(imp);
        }
    }

    // Similar to exercise_compress1_loop above.
    fn exercise_compress4_loop(implementation: Implementation) {
        let mut input_buffer = [0; 100 * BLOCKBYTES];
        paint_test_input(&mut input_buffer);
        let inputs = [
            &input_buffer[0..],
            &input_buffer[1..],
            &input_buffer[2..],
            &input_buffer[3..],
        ];
        exercise_cases(
            |invocations, blocks_per_invoc, count, last_block, last_node, stride, buffer_tail| {
                // Use the portable compress1_loop implementation to compute a
                // reference state for each input separately.
                let mut reference_states = [
                    input_state_words(0),
                    input_state_words(1),
                    input_state_words(2),
                    input_state_words(3),
                ];
                for i in 0..reference_states.len() {
                    portable::compress1_loop(
                        &mut reference_states[i],
                        inputs[i],
                        count,
                        u64_flag(last_block),
                        u64_flag(last_node),
                        invocations * blocks_per_invoc,
                        stride,
                        buffer_tail,
                    );
                }

                // Do the same thing in parallel with the
                // implementation under test under test, and
                // make sure the result is the same.
                let mut test_state0 = input_state_words(0);
                let mut test_state1 = input_state_words(1);
                let mut test_state2 = input_state_words(2);
                let mut test_state3 = input_state_words(3);
                for invocation in 0..invocations {
                    let is_last_invoc = invocation == invocations - 1;
                    let invoc_count =
                        count.wrapping_add((invocation * blocks_per_invoc * BLOCKBYTES) as u128);
                    let count_low = u64x4([invoc_count as u64; 4]);
                    let count_high = u64x4([(invoc_count >> 64) as u64; 4]);
                    let last_block = u64x4([u64_flag(is_last_invoc && last_block); 4]);
                    let last_node = u64x4([u64_flag(is_last_invoc && last_node); 4]);
                    let maybe_tail = u64x4([if is_last_invoc { buffer_tail as u64 } else { 0 }; 4]);
                    implementation.compress4_loop(
                        &mut test_state0,
                        &mut test_state1,
                        &mut test_state2,
                        &mut test_state3,
                        &inputs[0][invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        &inputs[1][invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        &inputs[2][invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        &inputs[3][invocation * blocks_per_invoc * stride * BLOCKBYTES..],
                        &count_low,
                        &count_high,
                        &last_block,
                        &last_node,
                        blocks_per_invoc,
                        stride,
                        &maybe_tail,
                    );
                }
                assert_eq!(reference_states[0], test_state0);
                assert_eq!(reference_states[1], test_state1);
                assert_eq!(reference_states[2], test_state2);
                assert_eq!(reference_states[3], test_state3);
            },
        );
    }

    fn exercise_compress4_loop_b(implementation: Implementation) {
        let mut input_buffer = [0; 100 * BLOCKBYTES];
        paint_test_input(&mut input_buffer);
        let inputs = [
            &input_buffer[0..],
            &input_buffer[1..],
            &input_buffer[2..],
            &input_buffer[3..],
        ];
        exercise_cases_b(
            |invocations, blocks_per_invoc, count, finalize, stride, buffer_tail| {
                // Use the portable compress1_loop implementation to compute a
                // reference state for each input separately.
                let total_blocks = invocations * blocks_per_invoc;
                let mut len = total_blocks * stride.padded_blockbytes();
                if buffer_tail != 0 {
                    len = len - stride.padded_blockbytes() + BLOCKBYTES - buffer_tail;
                }
                let mut reference_words = [
                    input_state_words(0),
                    input_state_words(1),
                    input_state_words(2),
                    input_state_words(3),
                ];
                for i in 0..reference_words.len() {
                    portable::compress1_loop_b(
                        Job::new(&mut reference_words[i], count, &inputs[i][..len], finalize),
                        stride,
                    );
                }

                // Do the same thing with the implementation under test under
                // test, and make sure the result is the same.
                let mut test_words = [
                    input_state_words(0),
                    input_state_words(1),
                    input_state_words(2),
                    input_state_words(3),
                ];
                let mut test_count = count;
                for invoc_num in 0..invocations {
                    let is_last_invoc = invoc_num == invocations - 1;
                    let offset = invoc_num * blocks_per_invoc * stride.padded_blockbytes();
                    let mut len = blocks_per_invoc * stride.padded_blockbytes();
                    if is_last_invoc && buffer_tail != 0 {
                        len = len - stride.padded_blockbytes() + BLOCKBYTES - buffer_tail;
                    }
                    let maybe_finalize = if !is_last_invoc {
                        Finalize::NotYet
                    } else {
                        finalize
                    };
                    let &mut [ref mut words0, ref mut words1, ref mut words2, ref mut words3] =
                        &mut test_words;
                    let mut jobs = [
                        Job::new(
                            words0,
                            test_count,
                            &inputs[0][offset..][..len],
                            maybe_finalize,
                        ),
                        Job::new(
                            words1,
                            test_count,
                            &inputs[1][offset..][..len],
                            maybe_finalize,
                        ),
                        Job::new(
                            words2,
                            test_count,
                            &inputs[2][offset..][..len],
                            maybe_finalize,
                        ),
                        Job::new(
                            words3,
                            test_count,
                            &inputs[3][offset..][..len],
                            maybe_finalize,
                        ),
                    ];
                    implementation.compress4_loop_b(&mut jobs, stride);
                    test_count = test_count.wrapping_add((blocks_per_invoc * BLOCKBYTES) as u128);
                    for job in &jobs {
                        assert!(job.input.is_empty());
                    }
                }
                assert_eq!(reference_words, test_words);
            },
        );
    }

    #[test]
    fn test_compress4_loop_portable() {
        exercise_compress4_loop(Implementation::portable());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress4_loop_sse41() {
        // Currently this just falls back to portable, but we test it anyway.
        if let Some(imp) = Implementation::sse41_if_supported() {
            exercise_compress4_loop(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress4_loop_avx2() {
        if let Some(imp) = Implementation::avx2_if_supported() {
            exercise_compress4_loop(imp);
        }
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_compress4_loop_avx2_b() {
        if let Some(imp) = Implementation::avx2_if_supported() {
            exercise_compress4_loop_b(imp);
        }
    }
}
