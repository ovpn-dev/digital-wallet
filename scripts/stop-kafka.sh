#!/bin/bash
# Stop Kafka and Zookeeper

echo "Stopping Kafka..."
pkill -f kafka.Kafka

sleep 2

echo "Stopping Zookeeper..."
pkill -f zookeeper

echo "✅ Kafka stopped"