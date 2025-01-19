

import argparse
import random

def generate_fake_dataset(count: int, seq_len: int, anomaly_rate: float = 0.0):
    x = []
    y = []
    start_index = 0
    for _ in range(count):
        i = start_index
        seq = []
        for _ in range(seq_len):
            if random.random() < anomaly_rate:
                seq.append(random.randint(0, seq_len))
            else:
                seq.append(i)
            i = (i + 1) % seq_len
        x.append(seq)
        y.append((i + 1) % seq_len)
        start_index = (start_index + 1) % seq_len
    return x, y

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Generate fake data for testing')
    parser.add_argument("--count", "-c", type=int, default=10, help="Number of fake data to generate")
    parser.add_argument("--seq-len", "-s", type=int, default=10, help="Length of each sequence")
    parser.add_argument("--anomaly-rate", "-a", type=float, default=0.1, help="Rate of anomalies in the data")

    args = parser.parse_args()

    print("Generating", args.count, "fake data")

    x, y = generate_fake_dataset(args.count, 10, args.anomaly_rate)

    print("x:", x)
    print("y:", y)

