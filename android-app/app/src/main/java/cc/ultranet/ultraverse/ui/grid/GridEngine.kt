package cc.ultranet.ultraverse.ui.grid

import cc.ultranet.ultraverse.wallet.NativeDilithium
import java.io.ByteArrayOutputStream
import java.nio.charset.StandardCharsets

/**
 * UltraGrid's engine, ported from `src/ultragrid/mod.rs`. The two must agree to the byte: the same
 * seed builds the same ten puzzles on the phone, on the server and for an auditor. That agreement is
 * pinned by `GridEngineTest` against vectors the Rust tests and an independent Python port also hold.
 *
 * Pure: no clock, no network, no OS randomness. The only outside call is SHA3-256.
 *
 * Do not "improve" the order of random draws, the tile tables or the byte layouts. Any change here that
 * the Rust engine does not make produces different puzzles, which is the failure this port exists to
 * prevent. Change both, bump [FORMAT_VERSION], and re-pin the vectors.
 */
internal object GridEngine {
    const val FORMAT_VERSION = 1
    const val LEVELS_PER_DAY = 10
    private const val MIN_PAR = 3
    private const val MAX_ATTEMPTS = 2_000

    // Directions, clockwise from north. The bit for direction `d` is `1 shl d`.
    private const val N = 0
    private const val E = 1
    private const val S = 2
    private const val W = 3
    private val DX = intArrayOf(0, 1, 0, -1)
    private val DY = intArrayOf(-1, 0, 1, 0)

    private fun bit(direction: Int): Int = 1 shl direction
    private fun opposite(direction: Int): Int = (direction + 2) % 4

    // --- hashing and byte helpers -----------------------------------------------------------

    private fun ascii(text: String): ByteArray = text.toByteArray(StandardCharsets.US_ASCII)

    private fun sha3(vararg parts: ByteArray): ByteArray {
        val joined = ByteArrayOutputStream()
        parts.forEach { joined.write(it) }
        return NativeDilithium.sha3_256(joined.toByteArray())
    }

    private fun leInt(value: Int): ByteArray = ByteArray(4) { (value ushr (8 * it)).toByte() }
    private fun leShort(value: Int): ByteArray = ByteArray(2) { (value ushr (8 * it)).toByte() }
    private fun leLong(value: Long): ByteArray = ByteArray(8) { (value ushr (8 * it)).toByte() }

    // --- tiles ------------------------------------------------------------------------------

    enum class TileKind(val code: Int, val period: Int) {
        Wall(0, 1),
        Terminal(1, 1),
        Straight(2, 2),
        Bend(3, 4),
        Tee(4, 4),
        Cross(5, 1);

        /** Whether a player may turn this tile at all. */
        val rotatable: Boolean get() = period > 1

        companion object {
            fun fromCode(code: Int): TileKind? = entries.firstOrNull { it.code == code }
        }
    }

    data class Tile(val kind: TileKind, val orientation: Int) {
        /** Open sides as a bitmask (N=1, E=2, S=4, W=8). */
        val arms: Int get() = armsOf(kind, orientation)

        fun turned(): Tile = Tile(kind, (orientation + 1) % kind.period)
    }

    /**
     * ```
     * Straight  o0 N+S   o1 E+W
     * Bend      o0 N+E   o1 E+S   o2 S+W   o3 W+N
     * Tee       o0 N+E+S o1 E+S+W o2 S+W+N o3 W+N+E   (the missing side moves clockwise)
     * ```
     */
    fun armsOf(kind: TileKind, orientation: Int): Int = when (kind) {
        TileKind.Wall -> 0
        TileKind.Terminal, TileKind.Cross -> 15
        TileKind.Straight -> if (orientation % 2 == 0) bit(N) or bit(S) else bit(E) or bit(W)
        TileKind.Bend -> {
            val o = orientation % 4
            bit(o) or bit((o + 1) % 4)
        }
        TileKind.Tee -> 15 and bit((W + orientation % 4) % 4).inv()
    }

    /** Clockwise turns to the nearest orientation opening every side in [needed], and that orientation. */
    private fun cheapestOrientation(tile: Tile, needed: Int): Pair<Int, Int>? {
        val period = tile.kind.period
        return (0 until period)
            .filter { armsOf(tile.kind, it) and needed == needed }
            .map { o -> Pair((o + period - tile.orientation) % period, o) }
            .minWithOrNull(compareBy({ it.first }, { it.second }))
    }

    // --- the random stream ------------------------------------------------------------------

    /**
     * SHA3-256 in counter mode, exactly as the Rust module documents it:
     * `block(i) = SHA3-256("UltraGrid/stream/v1" || seed || u32le(len(label)) || label || u64le(i))`,
     * words are u32le over the concatenated blocks, and `below(n)` rejects words at or above
     * `floor((2^32 - 1) / n) * n` so there is no modulo bias.
     */
    internal class Stream(private val seed: ByteArray, private val label: ByteArray) {
        private var counter = 0L
        private var block = ByteArray(32)
        private var position = 32

        private fun refill() {
            block = sha3(ascii("UltraGrid/stream/v1"), seed, leInt(label.size), label, leLong(counter))
            counter += 1
            position = 0
        }

        fun nextU32(): Long {
            if (position + 4 > 32) refill()
            var word = 0L
            for (i in 0 until 4) word = word or ((block[position + i].toLong() and 0xFF) shl (8 * i))
            position += 4
            return word
        }

        fun below(n: Int): Int {
            require(n > 0) { "below(0) has no answer" }
            val limit = (0xFFFF_FFFFL / n) * n
            while (true) {
                val word = nextU32()
                if (word < limit) return (word % n).toInt()
            }
        }
    }

    // --- puzzles ----------------------------------------------------------------------------

    class Puzzle(
        val level: Int,
        val width: Int,
        val height: Int,
        val cells: List<Tile>,
        val source: Int,
        val target: Int,
        /** Turns the reference route costs: always achievable, not claimed to be the minimum. */
        val par: Int,
    ) {
        /** `version, level, width, height, source u16le, target u16le, par u16le`, then `kind << 3 | orientation`. */
        fun canonicalBytes(): ByteArray {
            val out = ByteArrayOutputStream()
            out.write(byteArrayOf(FORMAT_VERSION.toByte(), level.toByte(), width.toByte(), height.toByte()))
            out.write(leShort(source))
            out.write(leShort(target))
            out.write(leShort(par))
            cells.forEach { out.write((it.kind.code shl 3) or it.orientation) }
            return out.toByteArray()
        }

        /** Identity of this exact puzzle; receipts and anchors name a puzzle by this. */
        fun id(): ByteArray = sha3(ascii("UltraGrid/puzzle-id/v1"), canonicalBytes())

        /** Turns allowed for a win: par plus a margin that grows with it. A policy constant. */
        val moveLimit: Int get() = par + maxOf(2, par / 4)
    }

    /** One entry of an answer: turn tile [cell] clockwise [turns] times. */
    data class Turn(val cell: Int, val turns: Int)

    class Generated(val puzzle: Puzzle, val reference: List<Turn>)

    /** Which tiles carry the signal, flooding out from [source]. A side carries it only if both tiles open onto it. */
    fun reachable(width: Int, height: Int, cells: List<Tile>, source: Int): BooleanArray {
        val seen = BooleanArray(cells.size)
        val stack = ArrayDeque<Int>()
        stack.addLast(source)
        seen[source] = true
        while (stack.isNotEmpty()) {
            val here = stack.removeLast()
            val x = here % width
            val y = here / width
            for (direction in 0 until 4) {
                val nx = x + DX[direction]
                val ny = y + DY[direction]
                if (nx < 0 || ny < 0 || nx >= width || ny >= height) continue
                val there = ny * width + nx
                if (seen[there]) continue
                if (cells[here].arms and bit(direction) != 0 &&
                    cells[there].arms and bit(opposite(direction)) != 0
                ) {
                    seen[there] = true
                    stack.addLast(there)
                }
            }
        }
        return seen
    }

    private fun connected(width: Int, height: Int, cells: List<Tile>, source: Int, target: Int): Boolean =
        reachable(width, height, cells, source)[target]

    /** Side length for a level: 4, 4, 5, 5, 6, 6, 7, 7, 8, 8. */
    fun side(level: Int): Int = 4 + level / 2

    private fun directionBetween(a: Int, b: Int, width: Int): Int {
        val ax = a % width
        val ay = a / width
        val bx = b % width
        val by = b / width
        return (0 until 4).first { ax + DX[it] == bx && ay + DY[it] == by }
    }

    /**
     * Cheapest route by random per-cell weights. Scans for the lowest unfinished distance in index
     * order and improves only on a strictly smaller distance, so ties resolve exactly as in Rust.
     */
    private fun route(
        width: Int,
        height: Int,
        blocked: BooleanArray,
        weights: IntArray,
        from: Int,
        to: Int,
    ): List<Int>? {
        val count = width * height
        val unreached = Long.MAX_VALUE
        val distance = LongArray(count) { unreached }
        val previous = IntArray(count) { -1 }
        val done = BooleanArray(count)
        distance[from] = 0
        while (true) {
            var best = -1
            var bestDistance = unreached
            for (index in 0 until count) {
                if (!done[index] && distance[index] < bestDistance) {
                    bestDistance = distance[index]
                    best = index
                }
            }
            if (best == -1) break
            done[best] = true
            if (best == to) break
            val x = best % width
            val y = best / width
            for (direction in 0 until 4) {
                val nx = x + DX[direction]
                val ny = y + DY[direction]
                if (nx < 0 || ny < 0 || nx >= width || ny >= height) continue
                val next = ny * width + nx
                if (blocked[next] || done[next]) continue
                val candidate = bestDistance + weights[next]
                if (candidate < distance[next]) {
                    distance[next] = candidate
                    previous[next] = best
                }
            }
        }
        if (distance[to] == unreached) return null
        val path = ArrayList<Int>()
        path.add(to)
        while (path.last() != from) path.add(previous[path.last()])
        path.reverse()
        return path
    }

    /** Build the puzzle for [level] from [seed]. Deterministic: same inputs, same bytes. */
    fun generate(seed: ByteArray, level: Int): Generated {
        require(level in 0 until LEVELS_PER_DAY) { "level $level is outside 0..${LEVELS_PER_DAY - 1}" }
        require(seed.size == 32) { "a seed is 32 bytes" }
        val stream = Stream(seed, ascii("puzzle/") + byteArrayOf(level.toByte()))
        repeat(MAX_ATTEMPTS) {
            attempt(stream, level)?.let { return it }
        }
        error("no acceptable puzzle for level $level after $MAX_ATTEMPTS attempts")
    }

    fun puzzle(seed: ByteArray, level: Int): Puzzle = generate(seed, level).puzzle

    private fun attempt(stream: Stream, level: Int): Generated? {
        val width = side(level)
        val height = width
        val count = width * height

        // Source on the left edge, target on the right edge.
        val source = stream.below(height) * width
        val target = stream.below(height) * width + (width - 1)

        // Walls: count/8 of them, never on a terminal.
        val blocked = BooleanArray(count)
        repeat(count / 8) {
            val cell = stream.below(count)
            if (cell != source && cell != target) blocked[cell] = true
        }

        val weights = IntArray(count) { 1 + stream.below(4) }
        val path = route(width, height, blocked, weights, source, target) ?: return null

        val cells = Array(count) { Tile(TileKind.Wall, 0) }

        // Decoys first, so the route's tiles overwrite whatever they land on.
        for (cell in 0 until count) {
            if (blocked[cell]) continue
            val roll = stream.below(100)
            val kind = when {
                roll < 25 -> TileKind.Straight
                roll < 65 -> TileKind.Bend
                roll < 90 -> TileKind.Tee
                else -> TileKind.Cross
            }
            cells[cell] = Tile(kind, stream.below(kind.period))
        }
        cells[source] = Tile(TileKind.Terminal, 0)
        cells[target] = Tile(TileKind.Terminal, 0)

        // The route's interior: the right arms, then a random scramble.
        val reference = ArrayList<Turn>()
        var par = 0
        for (window in 1 until path.size - 1) {
            val before = path[window - 1]
            val here = path[window]
            val after = path[window + 1]
            val needed = bit(directionBetween(here, before, width)) or bit(directionBetween(here, after, width))
            val oppositeArms = needed == (bit(N) or bit(S)) || needed == (bit(E) or bit(W))
            val roll = stream.below(100)
            val kind = if (oppositeArms) {
                when {
                    roll < 60 -> TileKind.Straight
                    roll < 75 -> TileKind.Cross
                    else -> TileKind.Tee
                }
            } else {
                if (roll < 65) TileKind.Bend else TileKind.Tee
            }
            val period = kind.period
            val valid = (0 until period).filter { armsOf(kind, it) and needed == needed }
            val planted = valid[stream.below(valid.size)]
            val scramble = stream.below(period)
            val tile = Tile(kind, (planted + scramble) % period)
            cells[here] = tile
            cheapestOrientation(tile, needed)?.let { (turns, _) ->
                if (turns > 0) {
                    reference.add(Turn(here, turns))
                    par += turns
                }
            }
        }

        // Reject a puzzle that is trivial, or already solved by the draw.
        if (par < MIN_PAR || connected(width, height, cells.toList(), source, target)) return null

        return Generated(Puzzle(level, width, height, cells.toList(), source, target, par), reference)
    }

    // --- verifying an answer ----------------------------------------------------------------

    sealed interface VerifyError {
        /** More entries than the grid has cells. */
        data object TooManyEntries : VerifyError
        data class CellOutOfRange(val cell: Int) : VerifyError

        /** A wall, a terminal or a cross: turning it is not a move. */
        data class NotRotatable(val cell: Int) : VerifyError

        /** `turns` must be `1 until period`; a full period is no move at all. */
        data class BadTurnCount(val cell: Int, val turns: Int) : VerifyError
        data class DuplicateCell(val cell: Int) : VerifyError
    }

    data class Outcome(
        /** Total quarter turns in the answer. */
        val moves: Int,
        val connected: Boolean,
        val par: Int,
        val limit: Int,
        /** Connected within the move limit: the only thing a prize may ever depend on. */
        val win: Boolean,
    )

    sealed interface Verification {
        data class Verified(val outcome: Outcome) : Verification
        data class Refused(val error: VerifyError) : Verification
    }

    /** Replay an answer on the puzzle's starting grid. Order does not matter, only the net turn per tile. */
    fun verify(puzzle: Puzzle, answer: List<Turn>): Verification {
        if (answer.size > puzzle.cells.size) return Verification.Refused(VerifyError.TooManyEntries)
        val cells = puzzle.cells.toMutableList()
        val touched = BooleanArray(cells.size)
        var moves = 0
        for (entry in answer) {
            if (entry.cell < 0 || entry.cell >= cells.size) {
                return Verification.Refused(VerifyError.CellOutOfRange(entry.cell))
            }
            val kind = cells[entry.cell].kind
            if (!kind.rotatable) return Verification.Refused(VerifyError.NotRotatable(entry.cell))
            if (entry.turns < 1 || entry.turns >= kind.period) {
                return Verification.Refused(VerifyError.BadTurnCount(entry.cell, entry.turns))
            }
            if (touched[entry.cell]) return Verification.Refused(VerifyError.DuplicateCell(entry.cell))
            touched[entry.cell] = true
            repeat(entry.turns) { cells[entry.cell] = cells[entry.cell].turned() }
            moves += entry.turns
        }
        val connected = connected(puzzle.width, puzzle.height, cells, puzzle.source, puzzle.target)
        val limit = puzzle.moveLimit
        return Verification.Verified(
            Outcome(moves = moves, connected = connected, par = puzzle.par, limit = limit, win = connected && moves <= limit),
        )
    }

    // --- the daily set ----------------------------------------------------------------------

    /** `YYYY-MM-DD`, digits only; the engine does not know calendars, the caller owns that. */
    fun validDate(date: String): Boolean =
        date.length == 10 && date[4] == '-' && date[7] == '-' &&
            date.withIndex().all { (i, c) -> i == 4 || i == 7 || c in '0'..'9' }

    /**
     * `SHA3-256("UltraGrid/day-seed/v1" || u32le(chainId) || date || previousRoot)`. The previous root is
     * the state root of the latest anchor mined before 00:00 UTC of the day, or 32 zero bytes.
     */
    fun dailySeed(chainId: Int, date: String, previousRoot: ByteArray): ByteArray? {
        if (!validDate(date) || previousRoot.size != 32) return null
        return sha3(ascii("UltraGrid/day-seed/v1"), leInt(chainId), ascii(date), previousRoot)
    }

    /** All ten puzzles of a day, easiest first. */
    fun dailySet(seed: ByteArray): List<Puzzle> = (0 until LEVELS_PER_DAY).map { puzzle(seed, it) }

    /** One hash over a set, in level order, for an anchor to commit to. */
    fun setCommitment(puzzles: List<Puzzle>): ByteArray {
        val parts = ArrayList<ByteArray>()
        parts.add(ascii("UltraGrid/set/v1"))
        parts.add(leInt(puzzles.size))
        puzzles.forEach { parts.add(it.id()) }
        return sha3(*parts.toTypedArray())
    }
}
