package fr.frohub.weft.features

import kotlin.math.PI
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.min

/**
 * What the remote control has to send, in order, between two ticks (about 60 a second): a move
 * or a scroll right after another of its kind is summed into it, so a fast drag leaves as one
 * `input.pointer` per tick and nothing is lost, while a click stays between the moves around it.
 * Not thread-safe: the screen adds and takes on the main thread.
 */
class InputQueue {
    private val packets = ArrayList<Packet>()

    fun move(dx: Double, dy: Double) {
        val last = packets.lastOrNull() as? Packet.InputPointer
        if (last?.dx != null) {
            packets[packets.lastIndex] = Packet.InputPointer(last.dx + dx, (last.dy ?: 0.0) + dy)
        } else {
            packets += Packet.InputPointer(dx, dy)
        }
    }

    fun scroll(x: Double, y: Double) {
        val last = packets.lastOrNull() as? Packet.InputPointer
        if (last?.scrollX != null) {
            packets[packets.lastIndex] = Packet.InputPointer(null, null, last.scrollX + x, (last.scrollY ?: 0.0) + y)
        } else {
            packets += Packet.InputPointer(null, null, x, y)
        }
    }

    /** The presenter spot ([spot] null hides it): only the newest of a run is sent. */
    fun point(spot: Spot?) {
        val packet = Packet.Presenter(null, spot)
        val last = packets.lastOrNull() as? Packet.Presenter
        if (last != null && last.action == null) packets[packets.lastIndex] = packet else packets += packet
    }

    /** A click or a key: sent as it is, after what came before. */
    fun add(packet: Packet) {
        packets += packet
    }

    /** Everything to send now, oldest first; empty when nothing happened since the last call. */
    fun take(): List<Packet> = packets.toList().also { packets.clear() }
}

/**
 * How many computer pixels one touchpad dp moves at [speedDpPerMs]: [sensitivity] when the
 * finger moves slowly (precise), up to three times more when it flicks (far).
 */
fun pointerGain(speedDpPerMs: Float, sensitivity: Float): Float =
    sensitivity * (1f + ACCELERATION * min(speedDpPerMs, MAX_ACCELERATED_SPEED))

private const val ACCELERATION = 1f
private const val MAX_ACCELERATED_SPEED = 2f

/**
 * What to send for a text field that went from [old] to [new] as the user typed: how many
 * backspaces (in characters as the user sees them, code points) and then which text.
 */
fun typedChange(old: String, new: String): Pair<Int, String> {
    var common = 0
    val shorter = min(old.length, new.length)
    while (common < shorter && old[common] == new[common]) common++
    // Never split a surrogate pair.
    if (common > 0 && Character.isHighSurrogate(old[common - 1])) common--
    return old.codePointCount(common, old.length) to new.substring(common)
}

/**
 * Where the phone points, from a rotation matrix (row-major 3x3, device to world, as
 * `SensorManager.getRotationMatrixFromVector` gives): the direction of its top edge, as a
 * heading (radians, clockwise from the reference) and an elevation (radians, up is positive).
 */
fun pointingAngles(rotation: FloatArray): Pair<Double, Double> {
    // The device's y axis (its top edge) in world coordinates: the matrix's second column.
    val east = rotation[1].toDouble()
    val north = rotation[4].toDouble()
    val up = rotation[7].toDouble().coerceIn(-1.0, 1.0)
    return atan2(east, north) to asin(up)
}

/**
 * The presenter spot for a phone that points at ([heading], [elevation]) after pointing at
 * ([heading0], [elevation0]) when the pointer button went down: the centre of the screen at
 * first, its edges [HALF_WIDTH] to the sides and [HALF_HEIGHT] up and down.
 */
fun presenterSpot(heading0: Double, elevation0: Double, heading: Double, elevation: Double): Spot {
    var turn = heading - heading0
    // The shortest way round.
    while (turn > PI) turn -= 2 * PI
    while (turn < -PI) turn += 2 * PI
    val x = (turn / HALF_WIDTH).coerceIn(-1.0, 1.0)
    // Raising the phone moves the spot up, towards -1.
    val y = ((elevation0 - elevation) / HALF_HEIGHT).coerceIn(-1.0, 1.0)
    return Spot(x, y)
}

private const val HALF_WIDTH = 25 * PI / 180
private const val HALF_HEIGHT = 15 * PI / 180
