#!/bin/bash
# Lightweight Kafka Setup for Development

KAFKA_VERSION="3.6.1"
SCALA_VERSION="2.13"
KAFKA_DIR="$HOME/kafka"

echo "Setting up Kafka ${KAFKA_VERSION}..."

# Download Kafka if not exists
if [ ! -d "$KAFKA_DIR" ]; then
    echo "Downloading Kafka..."
    cd ~
    wget https://archive.apache.org/dist/kafka/${KAFKA_VERSION}/kafka_${SCALA_VERSION}-${KAFKA_VERSION}.tgz
    tar -xzf kafka_${SCALA_VERSION}-${KAFKA_VERSION}.tgz
    mv kafka_${SCALA_VERSION}-${KAFKA_VERSION} kafka
    rm kafka_${SCALA_VERSION}-${KAFKA_VERSION}.tgz
fi

cd $KAFKA_DIR

# Create data directories
mkdir -p /tmp/zookeeper /tmp/kafka-logs

echo "Starting Zookeeper..."
bin/zookeeper-server-start.sh config/zookeeper.properties > /tmp/zookeeper.log 2>&1 &
ZOOKEEPER_PID=$!
echo "Zookeeper PID: $ZOOKEEPER_PID"

# Wait for Zookeeper to start
sleep 5

echo "Starting Kafka..."
bin/kafka-server-start.sh config/server.properties > /tmp/kafka.log 2>&1 &
KAFKA_PID=$!
echo "Kafka PID: $KAFKA_PID"

# Wait for Kafka to start
sleep 10

# Create topic
echo "Creating wallet-events topic..."
bin/kafka-topics.sh --create \
    --bootstrap-server localhost:9092 \
    --replication-factor 1 \
    --partitions 3 \
    --topic wallet-events

echo ""
echo "✅ Kafka setup complete!"
echo "Zookeeper PID: $ZOOKEEPER_PID (logs: /tmp/zookeeper.log)"
echo "Kafka PID: $KAFKA_PID (logs: /tmp/kafka.log)"
echo ""
echo "To stop Kafka later:"
echo "  kill $KAFKA_PID"
echo "  kill $ZOOKEEPER_PID"
echo ""
echo "Save these PIDs or use: pkill -f kafka"