import argparse
from tinygrad.tensor import Tensor
from tinygrad.nn import optim
from tinygrad.nn.state import get_parameters
import numpy as np
from fakedata import generate_fake_dataset

from loggpt import LogGPT, GPTConfig

# # Example of small GPT model
# vocab_size = 128  # Adjust based on tokenized log vocabulary
# embed_dim = 128   # Embedding dimension
# num_layers = 6    # Transformer layers
# num_heads = 4     # Number of attention heads
# seq_len = 128     # Max sequence length

# model = LogGPT(GPTConfig(
# 	block_size=seq_len,
# 	vocab_size=vocab_size,
# 	n_layer=num_layers,
# 	n_head=num_heads,
# 	embed_dim=embed_dim
# ))

# # def generate_data(batch_size, seq_len, vocab_size):
# #     """
# #     Generates input sequences (x) and target sequences (y) where y is the next token of x.
# #     """
# #     # Generate random integer sequences for inputs
# #     x = np.random.randint(0, vocab_size, size=(batch_size, seq_len)).astype(np.int32)
    
# #     # Shift x by one to create y
# #     y = np.roll(x, -1, axis=1)
    
# #     # Optionally, set the last token of y to a specific token (e.g., padding or end-of-sequence)
# #     y[:, -1] = 0  # Assuming 0 is the padding token
    
# #     return Tensor(x), Tensor(y)

# x, y = generate_fake_dataset(10_000, vocab_size, 0.01)
# x = Tensor(x)
# y = Tensor(y)

# print("x", x)
# print("y", y)

# batch_size = 16
# learning_rate = 1e-3
# optimizer = optim.SGD(get_parameters(model), lr=learning_rate)
# with Tensor.train():
# 	# Training loop
# 	for epoch in range(100):
# 		# x = generate_data(batch_size, seq_len, vocab_size)
# 		# y = generate_data(batch_size, seq_len, vocab_size)  # Next-token predictions
# 		# x, y = generate_data(batch_size, seq_len, vocab_size)

# 		# print("x", x.numpy())
# 		# print("y", y.numpy())

# 		x = x[:batch_size]
# 		y = y[:batch_size]

# 		# Forward pass
# 		logits, loss = model(x, y)

# 		# logits = logits[:, -1, :]
# 		# print("logits", logits.shape)
# 		# print("y", y.shape)
# 		# loss = logits.sparse_categorical_crossentropy(y)
# 		# print("loss", loss.numpy())
# 		# print(logits.numpy())
# 		# loss = logits.log_softmax(-1).mul(y.one_hot(vocab_size)).mean()

# 		#Backward pass
# 		optimizer.zero_grad()
# 		loss.backward()
# 		optimizer.step()

# 		print(f"Epoch {epoch + 1}, Loss: {loss.numpy()}")


if __name__ == '__main__':
	parser = argparse.ArgumentParser(description="Train a LogGPT model on fake data")
	parser.add_argument("--batch-size", "-b", type=int, default=16, help="Batch size for training")
	parser.add_argument("--learning-rate", "-l", type=float, default=1e-3, help="Learning rate for training")
	parser.add_argument("--epochs", "-e", type=int, default=100, help="Number of training epochs")
	parser.add_argument("--vocab-size", "-v", type=int, default=128, help="Size of the log vocabulary")
	parser.add_argument("--embed-dim", "-d", type=int, default=128, help="Embedding dimension")
	parser.add_argument("--num-layers", type=int, default=6, help="Number of transformer layers")
	parser.add_argument("--num-heads", type=int, default=4, help="Number of attention heads")
	parser.add_argument("--seq-len", "-s", type=int, default=128, help="Max sequence length")
	parser.add_argument("--anomaly-rate", "-a", type=float, default=0.01, help="Rate of anomalies in the data")
	parser.add_argument("--count", "-c", type=int, default=10_000, help="Number of fake data to generate")

	args = parser.parse_args()

	model = LogGPT(GPTConfig(
		block_size=args.seq_len,
		vocab_size=args.vocab_size,
		n_layer=args.num_layers,
		n_head=args.num_heads,
		embed_dim=args.embed_dim
	))

	x, y = generate_fake_dataset(args.count, args.vocab_size, args.anomaly_rate)
	x = Tensor(x)
	y = Tensor(y)

	optimizer = optim.SGD(get_parameters(model), lr=args.learning_rate)
	with Tensor.train():
		for epoch in range(args.epochs):
			x = x[:args.batch_size]
			y = y[:args.batch_size]

			logits, loss = model(x, y)

			optimizer.zero_grad()
			loss.backward()
			optimizer.step()

			print(f"Epoch {epoch + 1}, Loss: {loss.numpy()}")
