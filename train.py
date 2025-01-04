from tinygrad.tensor import Tensor
from tinygrad.nn import optim
from tinygrad.nn.state import get_parameters
import numpy as np

from gpt import GPT, GPTConfig

# Example of small GPT model
vocab_size = 128  # Adjust based on tokenized log vocabulary
embed_dim = 128   # Embedding dimension
num_layers = 6    # Transformer layers
num_heads = 4     # Number of attention heads
seq_len = 128     # Max sequence length

model = GPT(GPTConfig(
	block_size=seq_len,
	vocab_size=vocab_size,
	n_layer=num_layers,
	n_head=num_heads,
	embed_dim=embed_dim
))

def generate_data(batch_size, seq_len, vocab_size):
    """
    Generates input sequences (x) and target sequences (y) where y is the next token of x.
    """
    # Generate random integer sequences for inputs
    x = np.random.randint(0, vocab_size, size=(batch_size, seq_len)).astype(np.int32)
    
    # Shift x by one to create y
    y = np.roll(x, -1, axis=1)
    
    # Optionally, set the last token of y to a specific token (e.g., padding or end-of-sequence)
    y[:, -1] = 0  # Assuming 0 is the padding token
    
    return Tensor(x), Tensor(y)

batch_size = 16
learning_rate = 1e-3
optimizer = optim.SGD(get_parameters(model), lr=learning_rate)
with Tensor.train():
	# Training loop
	for epoch in range(10):
		# x = generate_data(batch_size, seq_len, vocab_size)
		# y = generate_data(batch_size, seq_len, vocab_size)  # Next-token predictions
		x, y = generate_data(batch_size, seq_len, vocab_size)

		# print("x", x.numpy())
		# print("y", y.numpy())

		# Forward pass
		logits, loss = model(x, y)
		print("loss", loss.numpy())
		# print(logits.numpy())
		# loss = logits.log_softmax(-1).mul(y.one_hot(vocab_size)).mean()

		# Backward pass
		optimizer.zero_grad()
		loss.backward()
		optimizer.step()

		print(f"Epoch {epoch + 1}, Loss: {loss.numpy()}")