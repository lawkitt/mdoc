package io.github.lawkitt.mdoc.ui

import io.github.lawkitt.mdoc.core.CaseColour
import io.github.lawkitt.mdoc.core.Category

/**
 * The category and case colours, light and dark (ADR 0087): the iOS system
 * colours' Android twins. Dark variants are lighter, so an underline or a
 * fill still stands out on a dark background.
 */
object Palette {
    fun category(category: Category, dark: Boolean): Int = when (category) {
        Category.Person -> if (dark) 0xFF4FA3FF else 0xFF0A6CD9
        Category.Organization -> if (dark) 0xFFC58CFF else 0xFF8A3FD1
        Category.Email -> if (dark) 0xFF5CD6E8 else 0xFF0A8A9E
        Category.Phone -> if (dark) 0xFF5FD97A else 0xFF1E8C3A
        Category.Address -> if (dark) 0xFFFFA64D else 0xFFC25E00
        Category.Date -> if (dark) 0xFFD1A877 else 0xFF8A5F2E
        Category.Identity -> if (dark) 0xFF8E8CFF else 0xFF4B49C7
        Category.Tax -> if (dark) 0xFFFF7AA8 else 0xFFC7255E
        Category.Bank -> if (dark) 0xFFFF6B61 else 0xFFC62A1F
        Category.Other -> if (dark) 0xFFA8A8AE else 0xFF6B6B70
    }.toInt()

    fun case(colour: CaseColour): Int = when (colour) {
        CaseColour.Red -> 0xFFE5484D
        CaseColour.Orange -> 0xFFF76B15
        CaseColour.Amber -> 0xFFFFB224
        CaseColour.Green -> 0xFF30A46C
        CaseColour.Teal -> 0xFF12A594
        CaseColour.Blue -> 0xFF0090FF
        CaseColour.Violet -> 0xFF8E4EC6
        CaseColour.Pink -> 0xFFD6409F
    }.toInt()

    data class Fill(val proposed: Float, val replaced: Float, val selected: Float)

    /** Candidate fills: stronger on dark, where a light tint all but disappears (ADR 0087). */
    fun fill(dark: Boolean) = if (dark) Fill(0.26f, 0.42f, 0.6f) else Fill(0.12f, 0.24f, 0.4f)

    /** `==highlight==`. */
    fun highlight(dark: Boolean): Int = if (dark) 0x52FFD60A else 0x66FFCC00
}
