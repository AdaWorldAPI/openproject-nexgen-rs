class TimeEntry < ApplicationRecord
  belongs_to :work_package
  belongs_to :user

  validates :hours, presence: true
end
