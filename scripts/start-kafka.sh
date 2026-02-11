#!/bin/bash
# Quick start Kafka (assumes it's already installed via setup-kafka.sh)

KAFKA_DIR="$HOME/kafka"

if [ ! -d "$KAFKA_DIR" ]; then
    echo "Kafka not found. Run ./setup-kafka.sh first"
    exit 1
fi

cd $KAFKA_DIR

echo "Starting Zookeeper..."
bin/zookeeper-server-start.sh config/zookeeper.properties > /tmp/zookeeper.log 2>&1 &
echo "Zookeeper started (PID: $!)"

sleep 5

echo "Starting Kafka..."
bin/kafka-server-start.sh config/server.properties > /tmp/kafka.log 2>&1 &
echo "Kafka started (PID: $!)"

echo ""
echo "✅ Kafka is running!"
echo "Logs: /tmp/zookeeper.log and /tmp/kafka.log"
echo ""
echo "To stop: ./stop-kafka.sh"