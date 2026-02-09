package com.example;

public final class App {
    public static void main(String[] args) {
        System.out.println(greet("Steel"));
    }

    public static String greet(String name) {
        if (name == null || name.isEmpty()) {
            return "Hello, world";
        }
        return "Hello, " + name;
    }
}
