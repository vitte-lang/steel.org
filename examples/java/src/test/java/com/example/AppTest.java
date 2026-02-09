package com.example;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class AppTest {
    @Test
    void greetDefaults() {
        assertEquals("Hello, world", App.greet(""));
    }

    @Test
    void greetName() {
        assertEquals("Hello, Steel", App.greet("Steel"));
    }
}
