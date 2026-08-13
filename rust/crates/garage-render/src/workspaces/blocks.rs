//! The workspace block allocator: which display owns which range of ids, remembered rather
//! than recomputed.
//!
//! Every display owns a block of `WORKSPACE_BLOCK` ids -- 1-10, 11-20, 21-30 -- and its
//! workspace count decides how many slots at the front of that block are persistent. So
//! 8/4/4 is 1-8, 11-14, 21-24, and lowering the first display's count to 6 is 1-6 with the
//! other two untouched. The block is as wide as the count may ever be, which is what makes it
//! a fixed allocation rather than a reservation that could run out: a display cannot ask for
//! an eleventh workspace, because there is no eleventh number key to reach it with.
//!
//! Packing the ranges instead, each display starting where the previous one stopped, was
//! tried and is wrong: Hyprland ids are global and a window lives on the id, so renumbering a
//! range dragged its windows onto another monitor -- changing one display's count visibly
//! rearranged the whole desktop. Gaps in the numbering cost nothing next to that: no key and
//! no navigation path addresses a global id directly, and an id in a gap is simply a
//! workspace that does not exist.
//!
//! # Nothing is ever reclaimed
//!
//! A display unplugged in the morning and plugged back in at night finds its workspaces
//! where it left them, which is worth more than keeping the numbers small -- and reclaiming
//! would eventually hand a returning display's block to a newcomer, which is precisely the
//! collision blocks exist to avoid. Deriving a block from a display's position in the
//! ordering has the same bug one step away: unplugging the second of three displays would
//! slide the third down a block and take its windows with it, the same renumbering triggered
//! by a cable rather than by a count. So a display keeps its block for as long as the
//! allocator file remembers it, and one seen for the first time takes the lowest block
//! nothing else holds -- adding a display therefore never disturbs an existing one.
//!
//! Keyed by connector, as the counts and the workspace rules already are. Moving a display to
//! another port renames it, and it starts again with a block of its own; `displays.toml`
//! carries a description that would survive the move, but one identity for a display beats a
//! second one only this allocator would understand.
//!
//! # The one sanctioned layer-2 write
//!
//! Renderers write layer 3 and never layer 2, with exactly one exception, and it is this
//! allocator. [`RenderCx`](crate::cx::RenderCx)'s own doc names it: a pure `garage render`
//! can write `workspace-blocks.toml` because the allocation has to survive the render that
//! produced it -- unremembered, it would be recomputed from the current display ordering,
//! and then unplugging display 2 of 3 slides display 3 into block 2 and drags its windows
//! with it. There is one allocator, and reading it means running it.
//!
//! That write does not weaken the no-lock invariant `render_idle()` depends on:
//! `workspace-blocks.toml` is one of the four host files precisely because it is *not*
//! `preferences.toml` -- this allocator is its single writer, that single-writer discipline
//! is what serialises it, and `PREFERENCES_LOCK` is not involved on either side. The
//! invariant is about the lock, not about the layer.
//!
//! Doc-only: the allocator's functions return `dict[str, int]` and write a file that is not a
//! `RenderStep` target on their own, so they are reached only through
//! [`crate::workspaces::plan::render_workspaces`] rather than carrying a stub of their own.
